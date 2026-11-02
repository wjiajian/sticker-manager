use super::*;
use zip::{CompressionMethod, ZipArchive};

const ENTRY_LIMIT: u64 = 100 * 1024 * 1024;
const ARCHIVE_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
const ENTRY_COUNT_LIMIT: usize = 10_000;

pub(super) fn is_zip(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
}
pub(super) fn open(path: &Path) -> std::result::Result<ZipArchive<File>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    ZipArchive::new(file).map_err(|e| format!("压缩包损坏或无法读取：{e}"))
}

fn filename(raw: &[u8], utf8: bool, fallback: impl FnOnce() -> String) -> String {
    if let Ok(s) = std::str::from_utf8(raw) {
        return s.into();
    }
    if utf8 {
        return String::from_utf8_lossy(raw).into_owned();
    }
    let (decoded, errors) = encoding_rs::GB18030.decode_without_bom_handling(raw);
    if !errors {
        decoded.into_owned()
    } else {
        fallback()
    }
}

/// 用实际读取量限流。超大条目只读到边界后一个字节，不信任 size()。
fn extract(
    reader: &mut impl Read,
    file: &mut impl Write,
    total: &mut u64,
    entry_limit: u64,
    archive_limit: u64,
) -> std::result::Result<(), String> {
    let mut entry = 0;
    let mut buffer = [0; 64 * 1024];
    loop {
        let allowance =
            (entry_limit.saturating_sub(entry)).min(archive_limit.saturating_sub(*total));
        let length = buffer.len().min(allowance.saturating_add(1) as usize);
        let n = reader
            .read(&mut buffer[..length])
            .map_err(|e| format!("条目损坏：{e}"))?;
        if n == 0 {
            return Ok(());
        }
        entry += n as u64;
        *total += n as u64;
        if *total > archive_limit {
            return Err("压缩包实际解压数据超过 2 GB，剩余条目未导入".into());
        }
        if entry > entry_limit {
            return Err("条目实际解压数据超过 100 MB".into());
        }
        file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
    }
}

#[derive(Clone, Copy)]
struct ArchiveLimits {
    entry: u64,
    total: u64,
    count: usize,
}
const LIMITS: ArchiveLimits = ArchiveLimits {
    entry: ENTRY_LIMIT,
    total: ARCHIVE_LIMIT,
    count: ENTRY_COUNT_LIMIT,
};

enum Entry {
    Ignored,
    Skipped,
    Image(NamedTempFile, String),
}

#[allow(clippy::too_many_arguments)]
pub(super) fn import(
    root: &Path,
    path: &Path,
    count: usize,
    report: &mut ImportReport,
    state: &mut ImportProgress,
    cancelled: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(ImportProgress),
    store: &mut dyn Store,
) {
    import_with_limits(
        root, path, count, report, state, cancelled, progress, store, LIMITS,
    );
}

#[allow(clippy::too_many_arguments)]
fn import_with_limits(
    root: &Path,
    path: &Path,
    count: usize,
    report: &mut ImportReport,
    state: &mut ImportProgress,
    cancelled: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(ImportProgress),
    store: &mut dyn Store,
    limits: ArchiveLimits,
) {
    let mut zip = match open(path) {
        Ok(zip) if zip.len() == count => zip,
        result => {
            report.failures.push(ImportFailure {
                path: path.into(),
                message: result
                    .err()
                    .unwrap_or_else(|| "压缩包在导入前发生变化，请重新导入".into()),
            });
            state.processed += count;
            progress(*state);
            return;
        }
    };
    let mut total = 0;
    let mut stopped: Option<String> = None;
    // 逐条解压后立即导入，同一时间只有一个临时文件。条目读完时校验 CRC，
    // 损坏的条目只算它自己失败；目录区读不出来时已在 open 里整包失败。
    for index in 0..count {
        if cancelled() {
            report.cancelled = true;
            return;
        }
        let mut display = path.to_path_buf();
        let result = (|| -> std::result::Result<Entry, String> {
            let entry = zip
                .by_index_data(index)
                .map_err(|e| format!("条目损坏：{e}"))?;
            let name = filename(
                entry.name_raw(),
                entry.flags().as_u16() & (1 << 11) != 0,
                || {
                    entry
                        .name()
                        .map(|s| s.into_owned())
                        .unwrap_or_else(|_| String::from_utf8_lossy(entry.name_raw()).into_owned())
                },
            );
            // 只使用文件名，绝不按压缩包中的路径落盘；同时兼容 Windows 的反斜杠。
            let name = name.replace('\\', "/");
            display = PathBuf::from(format!("{} › {name}", path.display()));
            if index >= limits.count {
                return Err("压缩包条目超过 10000 个，剩余条目未导入".into());
            }
            if let Some(reason) = &stopped {
                return Err(reason.clone());
            }
            if entry.is_dir()
                || name
                    .split('/')
                    .any(|p| p == "__MACOSX" || is_hidden(OsStr::new(p)))
            {
                return Ok(Entry::Ignored);
            }
            if entry.is_symlink() || is_zip(Path::new(&name)) {
                return Ok(Entry::Skipped);
            }
            if entry.encrypted() {
                return Err("不支持加密的压缩包".into());
            }
            if !matches!(
                entry.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            ) {
                return Err(format!("不支持的压缩方式：{:?}", entry.compression()));
            }
            let mut file =
                NamedTempFile::new_in(root.join("objects")).map_err(|e| e.to_string())?;
            let mut reader = zip.by_index(index).map_err(|e| format!("条目损坏：{e}"))?;
            if let Err(error) = extract(
                &mut reader,
                &mut file,
                &mut total,
                limits.entry,
                limits.total,
            ) {
                if total > limits.total {
                    stopped = Some(error.clone());
                }
                return Err(error);
            }
            let stem = Path::new(&name)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            Ok(Entry::Image(file, stem))
        })();
        match result {
            Ok(Entry::Ignored) => {}
            Ok(Entry::Skipped) => report.skipped += 1,
            Ok(Entry::Image(file, stem)) => {
                process(root, file.path(), &stem, &display, report, store)
            }
            Err(message) => report.failures.push(ImportFailure {
                path: display,
                message,
            }),
        }
        state.processed += 1;
        progress(*state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limits_count_actual_bytes_and_stop_without_reading_the_rest() {
        let mut total = 0;
        let mut data = &[0u8; 20][..];
        assert!(
            extract(&mut data, &mut Vec::new(), &mut total, 5, 12)
                .unwrap_err()
                .contains("100 MB")
        );
        assert_eq!((total, data.len()), (6, 14));
        let mut data = &[0u8; 20][..];
        assert!(
            extract(&mut data, &mut Vec::new(), &mut total, 20, 12)
                .unwrap_err()
                .contains("2 GB")
        );
        assert_eq!((total, data.len()), (13, 13));
    }
    #[test]
    fn filenames_prefer_utf8_then_gb18030() {
        let (gbk, _, errors) = encoding_rs::GB18030.encode("表情.png");
        assert!(!errors);
        assert_eq!(filename(&gbk, false, || "fallback".into()), "表情.png");
        assert_eq!(
            filename("猫猫.png".as_bytes(), false, || "fallback".into()),
            "猫猫.png"
        );
        assert_eq!(filename(&[0xff], false, || "fallback".into()), "fallback");
    }
    #[test]
    fn archive_limits_fail_only_oversize_entry_or_all_remaining_entries() {
        use crate::import::tests::make_zip;
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("limits.zip");
        make_zip(
            &zip,
            &[("a.txt", &[1; 12]), ("b.txt", &[2; 4]), ("c.txt", &[3; 4])],
        );
        let mut library = Library::open(&dir.path().join("library")).unwrap();
        let root = library.root.clone();
        let mut check = |limits: ArchiveLimits| {
            let mut report = ImportReport::default();
            let mut progress = ImportProgress {
                processed: 0,
                total: 3,
            };
            import_with_limits(
                &root,
                &zip,
                3,
                &mut report,
                &mut progress,
                &|| false,
                &mut |_| {},
                &mut library,
                limits,
            );
            assert_eq!(progress.processed, 3);
            report
        };
        let single = check(ArchiveLimits {
            entry: 10,
            total: 100,
            count: 100,
        });
        assert_eq!((single.failures.len(), single.skipped), (1, 2));
        assert!(single.failures[0].message.contains("100 MB"));
        let entire = check(ArchiveLimits {
            entry: 100,
            total: 13,
            count: 100,
        });
        assert_eq!((entire.failures.len(), entire.skipped), (2, 1));
        assert!(entire.failures.iter().all(|f| f.message.contains("2 GB")));
        let count = check(ArchiveLimits {
            entry: 100,
            total: 100,
            count: 1,
        });
        assert_eq!((count.failures.len(), count.skipped), (2, 1));
        assert!(count.failures.iter().all(|f| f.message.contains("10000")));
    }
}
