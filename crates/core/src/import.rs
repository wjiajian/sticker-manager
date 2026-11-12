use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use image::codecs::{gif::GifDecoder, png::PngDecoder, webp::WebPDecoder};
use image::{AnimationDecoder, DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::{Error, Library, Result, time::unix_millis};
mod archive;

const THUMBNAIL_SIZE: u32 = 256;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub added: u32,
    pub existing: u32,
    /// 恢复的表情也计入 existing，不计入 added。
    pub restored: u32,
    pub skipped: u32,
    pub failures: Vec<ImportFailure>,
    pub sticker_ids: Vec<String>,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFailure {
    pub path: PathBuf,
    pub message: String,
    /// 这一条代表几项未导入。压缩包超过限制时剩余条目合并成一条，其余都是 1。
    pub count: usize,
}

impl ImportFailure {
    fn new(path: PathBuf, message: String) -> Self {
        Self {
            path,
            message,
            count: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub processed: usize,
    pub total: usize,
}

/// 外壳使用此入口。枚举、读取、哈希、解码和缩略图都在仓库锁外。
pub fn import_paths_with_progress(
    library: &Mutex<Library>,
    paths: &[PathBuf],
    cancelled: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(ImportProgress),
) -> Result<ImportReport> {
    let mut store = Shared(library);
    let root = store.lock()?.root.clone();
    Ok(run(&root, paths, cancelled, progress, &mut store))
}

/// 导入写库的一端。准备阶段先查一次重，提交时在事务里再确认。
trait Store {
    /// 同哈希的表情已在库中、原图和缩略图都在时，准备阶段不必再解码。
    fn has_complete_files(&mut self, hash: &str, ext: &str) -> Result<bool>;
    fn commit(&mut self, prepared: Prepared, report: &mut ImportReport) -> Result<()>;
}

impl Store for Library {
    fn has_complete_files(&mut self, hash: &str, ext: &str) -> Result<bool> {
        let known: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM stickers WHERE hash=?1)",
            [hash],
            |r| r.get(0),
        )?;
        Ok(known
            && self
                .root
                .join("objects")
                .join(format!("{hash}.{ext}"))
                .is_file()
            && self
                .root
                .join("thumbs")
                .join(format!("{hash}.png"))
                .is_file())
    }
    fn commit(&mut self, prepared: Prepared, report: &mut ImportReport) -> Result<()> {
        self.commit_import(prepared, report)
    }
}

/// 桌面外壳用：查重和提交各自短暂持锁。
struct Shared<'a>(&'a Mutex<Library>);
impl Shared<'_> {
    fn lock(&self) -> Result<MutexGuard<'_, Library>> {
        self.0
            .lock()
            .map_err(|_| Error::Sync("仓库暂时无法访问".into()))
    }
}
impl Store for Shared<'_> {
    fn has_complete_files(&mut self, hash: &str, ext: &str) -> Result<bool> {
        self.lock()?.has_complete_files(hash, ext)
    }
    fn commit(&mut self, prepared: Prepared, report: &mut ImportReport) -> Result<()> {
        self.lock()?.commit_import(prepared, report)
    }
}

impl Library {
    /// 单线程调用方使用的同一导入路径；桌面外壳使用上面的分段加锁入口。
    pub fn import_paths(&mut self, paths: &[PathBuf]) -> ImportReport {
        let root = self.root.clone();
        run(&root, paths, &|| false, &mut |_| {}, self)
    }

    fn commit_import(&mut self, prepared: Prepared, report: &mut ImportReport) -> Result<()> {
        // Library -> files；清理端拿 files 后只 try_lock Library，不反向等待。
        // 准备文件仍是随机临时名，只有持有两把锁时才发布为哈希文件。
        let files = self.files.clone();
        let _files = files.lock().unwrap();
        let Prepared {
            original,
            mut thumbnail,
            hash,
            ext,
            format,
            name,
            category,
            mut dimensions,
            mut animated,
            byte_size,
        } = prepared;
        let tx = self.conn.transaction()?;
        let existing = tx
            .query_row(
                "SELECT id, deleted_at IS NOT NULL FROM stickers WHERE hash=?1",
                [&hash],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)),
            )
            .optional()?;
        if existing.is_none() && thumbnail.is_none() {
            // 准备时还有同图，提交前被彻底删除了：极少发生，在锁内补做解码。
            let (file, size, is_animated) = render(&self.root, original.path(), format)?;
            (thumbnail, dimensions, animated) = (Some(file), size, is_animated);
        }
        let id = existing
            .as_ref()
            .map(|(id, _)| id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        if existing.is_some() {
            tx.execute("UPDATE stickers SET deleted_at=NULL WHERE hash=?1", [&hash])?;
        } else {
            let category_id = match category.as_deref() {
                Some(folder) => crate::organization::category_for_folder(&tx, folder)?,
                None => None,
            };
            tx.execute(
                "INSERT INTO stickers(id,hash,ext,name,animated,width,height,byte_size,added_at,category_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![id, hash, ext, name, animated, dimensions.0, dimensions.1, byte_size, unix_millis()?, category_id],
            )?;
        }
        let object_path = self.root.join("objects").join(format!("{hash}.{ext}"));
        let thumbnail_path = self.root.join("thumbs").join(format!("{hash}.png"));
        let object_created = persist(original, &object_path)?;
        let mut thumbnail_created = false;
        let saved = (|| -> Result<()> {
            if let Some(file) = thumbnail {
                thumbnail_created = persist(file, &thumbnail_path)?;
            }
            tx.commit()?;
            Ok(())
        })();
        if let Err(error) = saved {
            if object_created {
                let _ = fs::remove_file(object_path);
            }
            if thumbnail_created {
                let _ = fs::remove_file(thumbnail_path);
            }
            return Err(error);
        }
        match existing {
            Some((_, deleted)) => {
                report.existing += 1;
                report.restored += u32::from(deleted);
            }
            None => report.added += 1,
        }
        report.sticker_ids.push(id);
        Ok(())
    }
}

struct Prepared {
    original: NamedTempFile,
    /// 已有同图且文件齐全时为 None，此时尺寸和动图标记不使用。
    thumbnail: Option<NamedTempFile>,
    hash: String,
    ext: &'static str,
    format: ImageFormat,
    name: String,
    /// 导入文件夹时的文件夹名；只给新表情设分类，已在库里的保持原样。
    category: Option<String>,
    dimensions: (u32, u32),
    animated: bool,
    byte_size: i64,
}

fn render(
    root: &Path,
    original: &Path,
    format: ImageFormat,
) -> Result<(NamedTempFile, (u32, u32), bool)> {
    let (image, animated) = decode(original, format)?;
    let dimensions = (image.width(), image.height());
    Ok((write_thumbnail(root, &image)?, dimensions, animated))
}

/// 在 `thumbs/` 里写一个临时缩略图，由调用方改名发布。只缩小不放大：
/// `DynamicImage::thumbnail` 会把小图放大到给定尺寸，所以边长先和原图取小。
pub(crate) fn write_thumbnail(root: &Path, image: &DynamicImage) -> Result<NamedTempFile> {
    let mut thumbnail = NamedTempFile::new_in(root.join("thumbs"))?;
    image
        .thumbnail(
            image.width().min(THUMBNAIL_SIZE),
            image.height().min(THUMBNAIL_SIZE),
        )
        .write_to(&mut thumbnail, ImageFormat::Png)?;
    thumbnail.as_file().sync_all()?;
    Ok(thumbnail)
}

fn prepare(
    root: &Path,
    path: &Path,
    name: &str,
    category: Option<&str>,
    store: &mut dyn Store,
) -> Result<Option<Prepared>> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    if supported_extension(reader.format()).is_none() {
        return Ok(None);
    }
    let mut source = reader.into_inner();
    let mut original = NamedTempFile::new_in(root.join("objects"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let len = source.read(&mut buffer)?;
        if len == 0 {
            break;
        }
        original.write_all(&buffer[..len])?;
        hasher.update(&buffer[..len]);
    }
    let hash = format!("{:x}", hasher.finalize());
    let byte_size =
        i64::try_from(original.as_file().metadata()?.len()).map_err(|_| Error::FileTooLarge)?;
    let format = ImageReader::open(original.path())?
        .with_guessed_format()?
        .format();
    let (Some(ext), Some(format)) = (supported_extension(format), format) else {
        return Ok(None);
    };
    let (thumbnail, dimensions, animated) = if store.has_complete_files(&hash, ext)? {
        (None, (0, 0), false)
    } else {
        let (file, dimensions, animated) = render(root, original.path(), format)?;
        (Some(file), dimensions, animated)
    };
    original.as_file().sync_all()?;
    Ok(Some(Prepared {
        original,
        thumbnail,
        hash,
        ext,
        format,
        name: name.into(),
        category: category.map(str::to_owned),
        dimensions,
        animated,
        byte_size,
    }))
}

enum Input {
    /// 第二项是图片所在文件夹的名字；直接选中的文件为 None。
    File(PathBuf, Option<String>),
    Zip(PathBuf, usize),
    Skip,
    Failed(PathBuf, String),
}
impl Input {
    fn count(&self) -> usize {
        match self {
            Self::Zip(_, n) => *n,
            _ => 1,
        }
    }
}

fn collect(root: &Path, paths: &[PathBuf], cancelled: &dyn Fn() -> bool) -> Vec<Input> {
    let mut inputs = Vec::new();
    let mut pending: Vec<_> = paths.iter().rev().cloned().map(|p| (p, true)).collect();
    while let Some((path, direct)) = pending.pop() {
        if cancelled() {
            break;
        }
        let result = (|| -> Result<()> {
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() || path.canonicalize()?.starts_with(root) {
                inputs.push(Input::Skip);
            } else if metadata.is_dir() {
                let mut children = Vec::new();
                for entry in fs::read_dir(&path)? {
                    match entry {
                        Ok(e) if is_hidden(&e.file_name()) => {}
                        Ok(e) => children.push(e.path()),
                        Err(e) => inputs.push(Input::Failed(path.clone(), e.to_string())),
                    }
                }
                children.sort();
                pending.extend(children.into_iter().rev().map(|p| (p, false)));
            } else if metadata.is_file() && direct && archive::is_zip(&path) {
                match archive::open(&path) {
                    Ok(zip) => inputs.push(Input::Zip(path.clone(), zip.len())),
                    Err(e) => inputs.push(Input::Failed(path.clone(), e)),
                }
            } else if metadata.is_file() {
                // 从文件夹里找到的图片归到它所在那一层文件夹同名的分类
                let folder = (!direct)
                    .then(|| path.parent().and_then(Path::file_name))
                    .flatten()
                    .map(|name| name.to_string_lossy().into_owned());
                inputs.push(Input::File(path.clone(), folder));
            } else {
                inputs.push(Input::Skip);
            }
            Ok(())
        })();
        if let Err(e) = result {
            inputs.push(Input::Failed(path, e.to_string()));
        }
    }
    inputs
}

fn run(
    root: &Path,
    paths: &[PathBuf],
    cancelled: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(ImportProgress),
    store: &mut dyn Store,
) -> ImportReport {
    let inputs = collect(root, paths, cancelled);
    let mut report = ImportReport {
        cancelled: cancelled(),
        ..ImportReport::default()
    };
    let mut state = ImportProgress {
        processed: 0,
        total: inputs.iter().map(Input::count).sum(),
    };
    progress(state);
    for input in inputs {
        if cancelled() {
            report.cancelled = true;
            break;
        }
        match input {
            Input::File(path, folder) => {
                let name = path.file_stem().unwrap_or_default().to_string_lossy();
                process(
                    root,
                    &path,
                    &name,
                    folder.as_deref(),
                    &path,
                    &mut report,
                    store,
                );
            }
            Input::Zip(path, count) => {
                archive::import(
                    root,
                    &path,
                    count,
                    &mut report,
                    &mut state,
                    cancelled,
                    progress,
                    store,
                );
                continue;
            }
            Input::Skip => report.skipped += 1,
            Input::Failed(path, message) => report.failures.push(ImportFailure::new(path, message)),
        }
        state.processed += 1;
        progress(state);
    }
    report.cancelled |= cancelled() && state.processed < state.total;
    let mut seen = BTreeSet::new();
    report.sticker_ids.retain(|id| seen.insert(id.clone()));
    progress(state);
    report
}
fn process(
    root: &Path,
    path: &Path,
    name: &str,
    category: Option<&str>,
    display: &Path,
    report: &mut ImportReport,
    store: &mut dyn Store,
) {
    let result = prepare(root, path, name, category, store).and_then(|prepared| match prepared {
        Some(p) => store.commit(p, report),
        None => {
            report.skipped += 1;
            Ok(())
        }
    });
    if let Err(e) = result {
        report
            .failures
            .push(ImportFailure::new(display.into(), e.to_string()));
    }
}

/// 遍历文件夹时直接忽略隐藏文件和系统生成的文件（.DS_Store、Thumbs.db 等），不计入跳过。
fn is_hidden(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    name.starts_with('.')
        || name.eq_ignore_ascii_case("Thumbs.db")
        || name.eq_ignore_ascii_case("desktop.ini")
}

fn supported_extension(format: Option<ImageFormat>) -> Option<&'static str> {
    match format {
        Some(ImageFormat::Png) => Some("png"),
        Some(ImageFormat::Jpeg) => Some("jpg"),
        Some(ImageFormat::Gif) => Some("gif"),
        Some(ImageFormat::WebP) => Some("webp"),
        _ => None,
    }
}

pub(crate) fn decode(path: &Path, format: ImageFormat) -> Result<(DynamicImage, bool)> {
    let reader = BufReader::new(File::open(path)?);
    match format {
        ImageFormat::Gif => {
            let mut decoder = GifDecoder::new(reader)?;
            decoder.set_limits(Limits::default())?;
            first_frame(decoder)
        }
        ImageFormat::Png => {
            let decoder = PngDecoder::with_limits(reader, Limits::default())?;
            if decoder.is_apng()? {
                first_frame(decoder.apng()?)
            } else {
                Ok((DynamicImage::from_decoder(decoder)?, false))
            }
        }
        ImageFormat::WebP => {
            let mut decoder = WebPDecoder::new(reader)?;
            decoder.set_limits(Limits::default())?;
            if decoder.has_animation() {
                first_frame(decoder)
            } else {
                Ok((DynamicImage::from_decoder(decoder)?, false))
            }
        }
        _ => {
            let mut decoder = ImageReader::with_format(reader, format).into_decoder()?;
            let orientation = decoder.orientation()?;
            let mut image = DynamicImage::from_decoder(decoder)?;
            image.apply_orientation(orientation);
            Ok((image, false))
        }
    }
}

fn first_frame<'a>(decoder: impl AnimationDecoder<'a>) -> Result<(DynamicImage, bool)> {
    let mut frames = decoder.into_frames();
    let first = frames.next().ok_or(Error::EmptyImage)??;
    let animated = frames.next().transpose()?.is_some();
    Ok((DynamicImage::ImageRgba8(first.into_buffer()), animated))
}

fn persist(file: NamedTempFile, path: &Path) -> Result<bool> {
    match file.persist_noclobber(path) {
        Ok(_) => Ok(true),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists && path.is_file() => {
            Ok(false)
        }
        Err(error) => Err(error.error.into()),
    }
}

#[cfg(test)]
mod tests;
