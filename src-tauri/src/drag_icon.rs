use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use image::{DynamicImage, RgbaImage};

const ICON_POINTS: u32 = 64;
const GAP_POINTS: u32 = 12;
/// 与导出副本一样保留七天，过期的图标在下次拖拽时重新生成。
const RETENTION: Duration = Duration::from_secs(7 * 86400);
const PREFIX: &str = "drag-icon-";
/// 改变布局或编码时更新版本，避免复用旧图标。
const LAYOUT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Anchor {
    Center,
    TopLeft,
}

const PLATFORM_ANCHOR: Anchor = if cfg!(target_os = "macos") {
    Anchor::Center
} else {
    Anchor::TopLeft
};

type IconResult<T> = Result<T, Box<dyn std::error::Error>>;

/// 系统临时目录中的拖拽图标缓存。
pub struct DragIconCache {
    root: PathBuf,
}

impl DragIconCache {
    pub fn new(root: PathBuf) -> Self {
        let cache = Self { root };
        cache.remove_expired(SystemTime::now());
        cache
    }

    pub fn icon_for(&self, thumbnail: &Path, scale: f64) -> PathBuf {
        match self.prepare(thumbnail, scale, PLATFORM_ANCHOR) {
            Ok(icon) => icon,
            Err(error) => {
                eprintln!("生成拖拽图标失败，改用缩略图：{error}");
                thumbnail.to_path_buf()
            }
        }
    }

    fn prepare(&self, thumbnail: &Path, scale: f64, anchor: Anchor) -> IconResult<PathBuf> {
        let stem = thumbnail
            .file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|stem| {
                !stem.is_empty()
                    && stem
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
            })
            .ok_or("无效的缩略图文件名")?;
        let scale = normalize_scale(scale);
        let target = self.root.join(file_name(stem, scale, anchor));
        if target.is_file() {
            return Ok(target);
        }

        self.remove_expired(SystemTime::now());
        fs::create_dir_all(&self.root)?;
        let icon = render(&image::open(thumbnail)?, scale, anchor);
        let bytes = encode_png(&icon, scale)?;
        let mut temp = tempfile::Builder::new()
            .prefix(PREFIX)
            .suffix(".partial")
            .tempfile_in(&self.root)?;
        temp.write_all(&bytes)?;
        temp.as_file().sync_all()?;
        match temp.persist(&target) {
            Ok(_) => Ok(target),
            Err(_) if target.is_file() => Ok(target),
            Err(error) => Err(error.into()),
        }
    }

    fn remove_expired(&self, now: SystemTime) {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        for entry in entries.flatten() {
            let expired = entry.file_name().to_string_lossy().starts_with(PREFIX)
                && entry.file_type().is_ok_and(|kind| kind.is_file())
                && entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .is_ok_and(|modified| {
                        now.duration_since(modified).unwrap_or_default() > RETENTION
                    });
            if expired {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

fn normalize_scale(scale: f64) -> f64 {
    if !scale.is_finite() || scale <= 0.0 {
        return 1.0;
    }
    (scale.clamp(1.0, 4.0) * 4.0).round() / 4.0
}

fn file_name(stem: &str, scale: f64, anchor: Anchor) -> String {
    let pct = (scale * 100.0).round() as u32;
    let a = match anchor {
        Anchor::Center => "c",
        Anchor::TopLeft => "t",
    };
    format!("{PREFIX}v{LAYOUT_VERSION}-{stem}-{pct}-{a}.png")
}

fn render(source: &DynamicImage, scale: f64, anchor: Anchor) -> RgbaImage {
    let px = |pt: u32| (f64::from(pt) * scale).round() as u32;
    let max_side = px(ICON_POINTS);
    let gap = px(GAP_POINTS);
    let icon = if source.width().max(source.height()) > max_side {
        source.thumbnail(max_side, max_side).to_rgba8()
    } else {
        source.to_rgba8()
    };
    let (width, height, offset) = match anchor {
        Anchor::Center => {
            let center = gap + max_side;
            (2 * center, 2 * center, center + gap)
        }
        Anchor::TopLeft => (gap + icon.width(), gap + icon.height(), gap),
    };
    let mut canvas = RgbaImage::new(width, height);
    image::imageops::overlay(&mut canvas, &icon, offset.into(), offset.into());
    canvas
}

fn encode_png(image: &RgbaImage, scale: f64) -> IconResult<Vec<u8>> {
    let mut buf = Vec::new();
    let mut encoder = png::Encoder::new(&mut buf, image.width(), image.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let ppm = (72.0 * scale / 0.0254).round() as u32;
    encoder.set_pixel_dims(Some(png::PixelDimensions {
        xppu: ppm,
        yppu: ppm,
        unit: png::Unit::Meter,
    }));
    let mut writer = encoder.write_header()?;
    writer.write_image_data(image.as_raw())?;
    writer.finish()?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn red(width: u32, height: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(width, height, Rgba([255, 0, 0, 255])))
    }

    fn thumbnail_fixture() -> (tempfile::TempDir, PathBuf, DragIconCache) {
        let dir = tempfile::tempdir().unwrap();
        let thumbs = dir.path().join("library/thumbs");
        fs::create_dir_all(&thumbs).unwrap();
        let path = thumbs.join("abc123.png");
        red(256, 256).save(&path).unwrap();
        let cache = DragIconCache::new(dir.path().join("cache"));
        (dir, path, cache)
    }

    #[test]
    fn large_thumbnail_fits_icon_box() {
        let source = red(256, 128);
        let icon = render(&source, 1.0, Anchor::TopLeft);
        assert_eq!(icon.dimensions(), (76, 44));
        assert_eq!(icon.get_pixel(12, 12)[3], 255);
        assert_eq!(icon.get_pixel(11, 11)[3], 0);
        assert_eq!(
            render(&source, 2.0, Anchor::TopLeft).dimensions(),
            (152, 88)
        );
    }

    #[test]
    fn aspect_ratio_is_preserved() {
        let icon = render(&red(100, 300), 1.0, Anchor::TopLeft);
        assert_eq!(icon.height(), 76);
        assert!((21..=22).contains(&(icon.width() - 12)));
    }

    #[test]
    fn small_thumbnail_is_not_upscaled() {
        let source = red(8, 8);
        let icon = render(&source, 2.0, Anchor::TopLeft);
        assert_eq!(icon.dimensions(), (32, 32));
        assert_eq!(icon.get_pixel(23, 23)[3], 0);
        for (x, y, pixel) in source.as_rgba8().unwrap().enumerate_pixels() {
            assert_eq!(icon.get_pixel(24 + x, 24 + y), pixel);
        }
    }

    #[test]
    fn center_anchor_places_icon_below_right_of_cursor() {
        let icon = render(&red(256, 256), 2.0, Anchor::Center);
        assert_eq!(icon.dimensions(), (304, 304));
        assert_eq!(icon.get_pixel(152, 152)[3], 0);
        assert_eq!(icon.get_pixel(175, 175)[3], 0);
        assert_eq!(icon.get_pixel(176, 176)[3], 255);
        assert_eq!(icon.get_pixel(303, 303)[3], 255);
    }

    #[test]
    fn png_records_density_for_scale() {
        let source = red(256, 256);
        for (scale, ppm) in [(1.0, 2835), (2.0, 5669)] {
            let icon = render(&source, scale, Anchor::Center);
            let png = encode_png(&icon, scale).unwrap();
            let reader = png::Decoder::new(std::io::Cursor::new(png))
                .read_info()
                .unwrap();
            let info = reader.info();
            assert_eq!((info.width, info.height), icon.dimensions());
            let dims = info.pixel_dims.unwrap();
            assert_eq!(
                (dims.xppu, dims.yppu, dims.unit),
                (ppm, ppm, png::Unit::Meter)
            );
        }
    }

    #[test]
    fn scale_is_normalized() {
        for (input, expected) in [
            (f64::NAN, 1.0),
            (f64::INFINITY, 1.0),
            (0.0, 1.0),
            (-1.0, 1.0),
            (0.5, 1.0),
            (1.3, 1.25),
            (1.5, 1.5),
            (2.0, 2.0),
            (9.0, 4.0),
        ] {
            assert_eq!(normalize_scale(input), expected);
        }
    }

    #[test]
    fn icon_is_cached_and_reused() {
        let (_dir, thumb, cache) = thumbnail_fixture();
        let icon = cache.prepare(&thumb, 2.0, Anchor::TopLeft).unwrap();
        fs::write(&icon, b"marker").unwrap();
        assert_eq!(cache.prepare(&thumb, 2.0, Anchor::TopLeft).unwrap(), icon);
        assert_eq!(fs::read(&icon).unwrap(), b"marker");
        assert_ne!(cache.prepare(&thumb, 1.0, Anchor::TopLeft).unwrap(), icon);
        assert_ne!(cache.prepare(&thumb, 2.0, Anchor::Center).unwrap(), icon);
    }

    #[test]
    fn cache_stays_out_of_library() {
        let (_dir, thumb, cache) = thumbnail_fixture();
        let icon = cache.prepare(&thumb, 1.0, Anchor::TopLeft).unwrap();
        assert!(icon.starts_with(&cache.root));
        assert_eq!(fs::read_dir(thumb.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn bad_thumbnail_falls_back() {
        let (dir, thumb, cache) = thumbnail_fixture();
        let missing = dir.path().join("missing.png");
        assert_eq!(cache.icon_for(&missing, 1.0), missing);
        fs::write(&thumb, b"not png").unwrap();
        assert_eq!(cache.icon_for(&thumb, 1.0), thumb);
        assert_eq!(fs::read_dir(&cache.root).unwrap().count(), 0);
        for invalid in ["a..b.png", "bad name.png"] {
            let path = thumb.with_file_name(invalid);
            red(8, 8).save(&path).unwrap();
            assert!(cache.prepare(&path, 1.0, Anchor::TopLeft).is_err());
        }
    }

    #[test]
    fn removes_only_expired_icons() {
        let (_dir, thumb, cache) = thumbnail_fixture();
        let icon = cache.prepare(&thumb, 1.0, Anchor::TopLeft).unwrap();
        let partial = cache.root.join("drag-icon-leftover.partial");
        let unrelated = cache.root.join("其他.png");
        fs::write(&partial, b"incomplete").unwrap();
        fs::write(&unrelated, b"keep").unwrap();
        let now = SystemTime::now();
        cache.remove_expired(now);
        assert!(icon.is_file() && partial.is_file() && unrelated.is_file());
        cache.remove_expired(now + RETENTION + Duration::from_secs(60));
        assert!(!icon.exists() && !partial.exists());
        assert!(unrelated.is_file());
    }

    #[test]
    fn unusable_root_falls_back() {
        let (_dir, thumb, cache) = thumbnail_fixture();
        fs::write(&cache.root, b"file").unwrap();
        assert_eq!(cache.icon_for(&thumb, 2.0), thumb);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn nsimage_honors_png_density() {
        use objc2::AnyThread;
        use objc2_app_kit::NSImage;
        use objc2_foundation::NSString;

        let (_dir, thumb, cache) = thumbnail_fixture();
        let path = cache.prepare(&thumb, 2.0, Anchor::Center).unwrap();
        let img = NSImage::initByReferencingFile(
            NSImage::alloc(),
            &NSString::from_str(&path.to_string_lossy()),
        )
        .unwrap();
        let size = img.size();
        assert!(
            (size.width - 152.0).abs() < 0.5 && (size.height - 152.0).abs() < 0.5,
            "NSImage size: {size:?}"
        );
    }
}
