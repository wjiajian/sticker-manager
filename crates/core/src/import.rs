use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use image::codecs::{gif::GifDecoder, png::PngDecoder, webp::WebPDecoder};
use image::{AnimationDecoder, DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::{Error, Library, Result};

const THUMBNAIL_SIZE: u32 = 256;

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub added: u32,
    pub existing: u32,
    /// 恢复的表情也计入 existing，不计入 added。
    pub restored: u32,
    pub skipped: u32,
    pub failures: Vec<ImportFailure>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFailure {
    pub path: PathBuf,
    pub message: String,
}

impl Library {
    /// 每个文件独立保存，单个文件失败不撤销已经成功的导入。
    pub fn import_paths(&mut self, paths: &[PathBuf]) -> ImportReport {
        let mut report = ImportReport::default();
        let mut pending: Vec<_> = paths.iter().rev().cloned().collect();
        while let Some(path) = pending.pop() {
            if let Err(error) = self.import_path(&path, &mut pending, &mut report) {
                report.failures.push(ImportFailure {
                    path,
                    message: error.to_string(),
                });
            }
        }
        report
    }

    fn import_path(
        &mut self,
        path: &Path,
        pending: &mut Vec<PathBuf>,
        report: &mut ImportReport,
    ) -> Result<()> {
        let metadata = fs::symlink_metadata(path)?;
        // 不遍历链接和仓库自身，避免目录循环及重复导入内部文件。
        if metadata.file_type().is_symlink() || path.canonicalize()?.starts_with(&self.root) {
            report.skipped += 1;
        } else if metadata.is_dir() {
            let mut children = Vec::new();
            for entry in fs::read_dir(path)? {
                match entry {
                    Ok(entry) if is_hidden(&entry.file_name()) => {}
                    Ok(entry) => children.push(entry.path()),
                    Err(error) => report.failures.push(ImportFailure {
                        path: path.to_path_buf(),
                        message: error.to_string(),
                    }),
                }
            }
            children.sort();
            pending.extend(children.into_iter().rev());
        } else if metadata.is_file() {
            self.import_file(path, report)?;
        } else {
            report.skipped += 1;
        }
        Ok(())
    }

    fn import_file(&mut self, path: &Path, report: &mut ImportReport) -> Result<()> {
        let reader = ImageReader::open(path)?.with_guessed_format()?;
        if supported_extension(reader.format()).is_none() {
            report.skipped += 1;
            return Ok(());
        }

        let mut source = reader.into_inner();
        let mut original = NamedTempFile::new_in(self.root.join("objects"))?;
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
        // 从副本读取元数据，保证哈希、缩略图、原文件来自同一份内容。
        let format = ImageReader::open(original.path())?
            .with_guessed_format()?
            .format();
        let Some(ext) = supported_extension(format) else {
            report.skipped += 1;
            return Ok(());
        };
        let existing = self
            .conn
            .query_row(
                "SELECT deleted_at IS NOT NULL FROM stickers WHERE hash = ?1",
                [&hash],
                |row| row.get::<_, bool>(0),
            )
            .optional()?;
        let object_path = self.root.join("objects").join(format!("{hash}.{ext}"));
        let thumbnail_path = self.root.join("thumbs").join(format!("{hash}.png"));
        let mut thumbnail = None;
        let mut dimensions = (0, 0);
        let mut animated = false;
        if existing.is_none() || !thumbnail_path.is_file() || !object_path.is_file() {
            let (image, is_animated) = decode(original.path(), format.unwrap())?;
            dimensions = (image.width(), image.height());
            animated = is_animated;
            let mut file = NamedTempFile::new_in(self.root.join("thumbs"))?;
            image
                .thumbnail(
                    image.width().min(THUMBNAIL_SIZE),
                    image.height().min(THUMBNAIL_SIZE),
                )
                .write_to(&mut file, ImageFormat::Png)?;
            file.as_file().sync_all()?;
            thumbnail = Some(file);
        }
        original.as_file().sync_all()?;

        let tx = self.conn.transaction()?;
        if existing.is_some() {
            tx.execute(
                "UPDATE stickers SET deleted_at = NULL WHERE hash = ?1",
                [&hash],
            )?;
        } else {
            let added_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| Error::InvalidTime)?
                .as_millis();
            let added_at = i64::try_from(added_at).map_err(|_| Error::InvalidTime)?;
            let name = path.file_stem().unwrap_or_default().to_string_lossy();
            tx.execute(
                "INSERT INTO stickers (id, hash, ext, name, animated, width, height, byte_size, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![Uuid::new_v4().to_string(), hash, ext, name, animated, dimensions.0, dimensions.1, byte_size, added_at],
            )?;
        }

        // 先保存完整文件，再提交数据库；提交失败时只清理本次新建的文件。
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
            Some(deleted) => {
                report.existing += 1;
                report.restored += u32::from(deleted);
            }
            None => report.added += 1,
        }
        Ok(())
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
