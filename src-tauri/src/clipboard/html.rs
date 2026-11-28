//! HTML is used only for clipboard import. No network access or remote file hosts.
use crate::staging::{MAX_BYTES, MAX_ITEMS, Staging};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{collections::HashSet, path::PathBuf};

fn sources(html: &str) -> Vec<String> {
    let bytes = html.as_bytes();
    let mut sources = Vec::new();
    let mut i = 0;
    while i < bytes.len() && sources.len() < MAX_ITEMS {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        i += 1;
        if !bytes
            .get(i..i + 3)
            .is_some_and(|b| b.eq_ignore_ascii_case(b"img"))
        {
            continue;
        }
        i += 3;
        if !bytes
            .get(i)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>' || *b == b'/')
        {
            continue;
        }
        while i < bytes.len() && bytes[i] != b'>' {
            while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
                i += 1;
            }
            let start = i;
            while i < bytes.len()
                && !bytes[i].is_ascii_whitespace()
                && !matches!(bytes[i], b'=' | b'>' | b'/')
            {
                i += 1;
            }
            let key = &bytes[start..i];
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if bytes.get(i) != Some(&b'=') {
                if i == start {
                    break;
                }
                continue;
            }
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let quote = bytes.get(i).copied().filter(|b| matches!(b, b'\'' | b'"'));
            if quote.is_some() {
                i += 1;
            }
            let start = i;
            while i < bytes.len()
                && match quote {
                    Some(q) => bytes[i] != q,
                    None => !bytes[i].is_ascii_whitespace() && bytes[i] != b'>',
                }
            {
                i += 1;
            }
            if key.eq_ignore_ascii_case(b"src") {
                sources.push(html[start..i].replace("&amp;", "&"));
                if sources.len() >= MAX_ITEMS {
                    break;
                }
            }
            if quote.is_some() && i < bytes.len() {
                i += 1;
            }
        }
    }
    sources
}
pub fn local_file(src: &str) -> Option<PathBuf> {
    let url = url::Url::parse(src).ok()?;
    if url.scheme() != "file" || !matches!(url.host_str(), None | Some("localhost")) {
        return None;
    }
    let path = url.to_file_path().ok()?;
    // A file URL with an empty host can still decode to a UNC path on Windows.
    if path.to_string_lossy().starts_with("\\\\") || path.to_string_lossy().starts_with("//") {
        return None;
    }
    Some(path)
}
pub struct Images {
    pub paths: Vec<PathBuf>,
    pub remote_only: bool,
}
pub fn images(html: &str, staged: &mut Staging) -> Images {
    let mut paths = Vec::new();
    let mut remote = false;
    let mut seen = HashSet::new();
    for src in sources(html) {
        if !seen.insert(src.clone()) {
            continue;
        }
        if let Some((prefix, data)) = src.split_once(',')
            && matches!(
                prefix.to_ascii_lowercase().as_str(),
                "data:image/png;base64"
                    | "data:image/gif;base64"
                    | "data:image/jpeg;base64"
                    | "data:image/jpg;base64"
                    | "data:image/webp;base64"
            )
        {
            if data.len() <= MAX_BYTES * 2
                && let Ok(bytes) = STANDARD.decode(
                    data.bytes()
                        .filter(|b| !b.is_ascii_whitespace())
                        .collect::<Vec<_>>(),
                )
            {
                match staged.add_bytes(&bytes) {
                    Ok(path) => paths.push(path),
                    Err(error) => staged.fail("图片", error, 1),
                }
            }
        } else if let Some(path) = local_file(&src) {
            if path.is_file() {
                paths.push(path);
            }
        } else if !src.to_ascii_lowercase().starts_with("data:") {
            remote = true;
        }
    }
    Images {
        remote_only: remote && paths.is_empty(),
        paths,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::staging::Origin;

    #[test]
    fn html_only_local_and_data_sources() {
        let root = tempfile::tempdir().unwrap();
        let mut staging = Staging::new(root.path(), Origin::Clipboard).unwrap();
        let result = images(
            "<IMG SRC='data:image/gif;base64,R0lGODlh'><img src=data:image/gif;base64,R0lGODlh><img src=\"https://example.com/x?a=1&amp;b=2\"><img src='file://server/share/a.png'>",
            &mut staging,
        );
        assert_eq!(result.paths.len(), 1);
        assert!(!result.remote_only);
        assert!(images("<img src=//example.com/a.png>", &mut staging).remote_only);
        assert!(local_file("file://server/share/a.png").is_none());
        assert_eq!(sources("<img alt='src=x' SRC=\"a&amp;b\">"), ["a&b"]);
    }

    #[test]
    fn html_handles_mime_variants_whitespace_invalid_base64_and_cf_html() {
        let root = tempfile::tempdir().unwrap();
        let mut staging = Staging::new(root.path(), Origin::Clipboard).unwrap();
        let png = STANDARD.encode(b"\x89PNG\r\n\x1a\n");
        let jpg = STANDARD.encode([0xff, 0xd8, 0xff, 0xe0]);
        let webp = STANDARD.encode(b"RIFF....WEBP");
        let html = format!(
            "Version:0.9\r\nStartHTML:00000097\r\n<html><body>\
             <img src=\"DATA:IMAGE/PNG;BASE64, {a}\n{b}\">\
             <img src='data:image/jpeg;base64,{jpg}'>\
             <img src=data:image/webp;base64,{webp}>\
             <img src=\"data:image/png;base64,***not-base64***\">\
             <img src=\"blob:https://example.com/1\">\
             <img src=\"cid:inline-1\">\
             </body></html>",
            a = &png[..4],
            b = &png[4..],
        );
        let result = images(&html, &mut staging);
        assert_eq!(result.paths.len(), 3);
        assert!(!result.remote_only);
        assert!(
            images(
                "<img src='blob:x'><img src='cid:y'><img src='/rel.png'>",
                &mut staging
            )
            .remote_only
        );
    }

    #[test]
    fn html_accepts_localhost_percent_encoded_file_and_rejects_directories() {
        let root = tempfile::tempdir().unwrap();
        let mut staging = Staging::new(root.path(), Origin::Clipboard).unwrap();
        let subdir = root.path().join("表情 folder");
        std::fs::create_dir(&subdir).unwrap();
        let file = subdir.join("可爱 1.gif");
        std::fs::write(&file, b"GIF89a").unwrap();
        let file_url = url::Url::from_file_path(&file).unwrap();
        let localhost_url = file_url
            .as_str()
            .replacen("file:///", "file://localhost/", 1);
        let dir_url = url::Url::from_directory_path(&subdir).unwrap();
        let html = format!("<img src=\"{localhost_url}\"><img src=\"{dir_url}\">");
        let result = images(&html, &mut staging);
        assert_eq!(result.paths, [file]);
        assert!(!result.remote_only);
    }

    #[test]
    fn html_caps_sources_at_max_items() {
        let html: String = (0..MAX_ITEMS + 20)
            .map(|i| format!("<img src=\"https://example.com/{i}.png\">"))
            .collect();
        assert_eq!(sources(&html).len(), MAX_ITEMS);
    }
}
