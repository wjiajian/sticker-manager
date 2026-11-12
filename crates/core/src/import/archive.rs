use super::*;
use zip::{CompressionMethod, ZipArchive, read::ZipFileEntry};

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
            return Err("压缩包实际解压数据超过 2 GB".into());
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
            report.failures.push(ImportFailure::new(
                path.into(),
                result
                    .err()
                    .unwrap_or_else(|| "压缩包在导入前发生变化，请重新导入".into()),
            ));
            state.processed += count;
            progress(*state);
            return;
        }
    };
    let mut total = 0;
    // 逐条解压后立即导入，同一时间只有一个临时文件。条目读完时校验 CRC，
    // 损坏的条目只算它自己失败；目录区读不出来时已在 open 里整包失败。
    for index in 0..count {
        if cancelled() {
            report.cancelled = true;
            return;
        }
        // 超过限制后剩余条目合并成一条失败，免得几十万条目各占一行。
        if index >= limits.count {
            let remaining = importable(&zip, index..count);
            if remaining > 0 {
                report.failures.push(ImportFailure {
                    path: path.into(),
                    message: format!(
                        "压缩包条目超过 {ENTRY_COUNT_LIMIT} 个，剩余 {remaining} 个条目未导入"
                    ),
                    count: remaining,
                });
            }
            state.processed += count - index;
            progress(*state);
            return;
        }
        let mut display = path.to_path_buf();
        let result = (|| -> std::result::Result<Entry, String> {
            let entry = zip
                .by_index_data(index)
                .map_err(|e| format!("条目损坏：{e}"))?;
            let name = entry_name(&entry);
            display = PathBuf::from(format!("{} › {name}", path.display()));
            if ignored(&entry, &name) {
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
            extract(
                &mut reader,
                &mut file,
                &mut total,
                limits.entry,
                limits.total,
            )?;
            let stem = Path::new(&name)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            Ok(Entry::Image(file, stem))
        })();
        match result {
            // 解压总量越线：这一项和之后的条目都不再导入。
            Err(message) if total > limits.total => {
                let remaining = 1 + importable(&zip, index + 1..count);
                report.failures.push(ImportFailure {
                    path: display,
                    message: format!("{message}，从这一项起 {remaining} 个条目未导入"),
                    count: remaining,
                });
                state.processed += count - index;
                progress(*state);
                return;
            }
            Err(message) => report.failures.push(ImportFailure::new(display, message)),
            Ok(Entry::Ignored) => {}
            Ok(Entry::Skipped) => report.skipped += 1,
            Ok(Entry::Image(file, stem)) => {
                process(root, file.path(), &stem, None, &display, report, store)
            }
        }
        state.processed += 1;
        progress(*state);
    }
}

fn entry_name(entry: &ZipFileEntry<'_>) -> String {
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
    name.replace('\\', "/")
}

/// 目录、__MACOSX 和隐藏文件直接忽略，不计入跳过或失败。
fn ignored(entry: &ZipFileEntry<'_>, name: &str) -> bool {
    entry.is_dir()
        || name
            .split('/')
            .any(|p| p == "__MACOSX" || is_hidden(OsStr::new(p)))
}

/// 范围内本来要处理的条目数，只读目录区，不解压。读不出的条目算在内，它本来也会记为失败。
fn importable(zip: &ZipArchive<File>, indices: std::ops::Range<usize>) -> usize {
    indices
        .filter(|&index| {
            zip.by_index_data(index)
                .map_or(true, |entry| !ignored(&entry, &entry_name(&entry)))
        })
        .count()
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
    fn archive_limits_fail_only_oversize_entry_or_merge_all_remaining_entries() {
        use crate::import::tests::make_zip;
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("limits.zip");
        // 末尾两条本来就会忽略，合并后的未导入数量不算它们。
        make_zip(
            &zip,
            &[
                ("a.txt", &[1; 12]),
                ("b.txt", &[2; 4]),
                ("c.txt", &[3; 4]),
                ("__MACOSX/._c.txt", &[4; 1]),
                (".DS_Store", &[5; 1]),
            ],
        );
        let mut library = Library::open(&dir.path().join("library")).unwrap();
        let root = library.root.clone();
        let mut check = |limits: ArchiveLimits| {
            let mut report = ImportReport::default();
            let mut progress = ImportProgress {
                processed: 0,
                total: 5,
            };
            let mut reported = 0;
            import_with_limits(
                &root,
                &zip,
                5,
                &mut report,
                &mut progress,
                &|| false,
                &mut |_| reported += 1,
                &mut library,
                limits,
            );
            assert_eq!(progress.processed, 5);
            (report, reported)
        };
        let (single, _) = check(ArchiveLimits {
            entry: 10,
            total: 100,
            count: 100,
        });
        assert_eq!((single.failures.len(), single.skipped), (1, 2));
        assert_eq!(single.failures[0].count, 1);
        assert!(single.failures[0].message.contains("100 MB"));
        let (entire, reported) = check(ArchiveLimits {
            entry: 100,
            total: 13,
            count: 100,
        });
        assert_eq!((entire.failures.len(), entire.skipped), (1, 1));
        assert_eq!(entire.failures[0].count, 2);
        assert!(entire.failures[0].path.ends_with("limits.zip › b.txt"));
        assert!(entire.failures[0].message.contains("2 GB"));
        assert!(entire.failures[0].message.contains("2 个条目未导入"));
        // a 一次，越线后剩余条目一次。
        assert_eq!(reported, 2);
        let (count, _) = check(ArchiveLimits {
            entry: 100,
            total: 100,
            count: 1,
        });
        assert_eq!((count.failures.len(), count.skipped), (1, 1));
        assert_eq!(count.failures[0].count, 2);
        assert!(count.failures[0].message.contains("10000"));
        assert!(count.failures[0].message.contains("剩余 2 个条目未导入"));
        let (only_ignored_left, _) = check(ArchiveLimits {
            entry: 100,
            total: 100,
            count: 3,
        });
        assert_eq!(
            (only_ignored_left.failures.len(), only_ignored_left.skipped),
            (0, 3)
        );
    }
}
