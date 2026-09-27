//! Web images for the Markdown preview: downloads into a disk cache, and the
//! record of which folders the user allowed to load them.
//!
//! LightLine only goes online when the user asks, so nothing here runs
//! unless the preview's "Load images" bar was clicked for that folder or
//! `markdownLoadRemoteImages` is on. Downloads are bounded in time and size,
//! and a cached image is reused without touching the network again.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_BYTES: u64 = 5 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(15);

fn lightline_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("LightLine"))
}

// FNV-1a: a stable file name for a URL (std's hasher may change between
// Rust versions, which would orphan the cache).
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn is_web(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

fn cache_path_in(dir: &Path, url: &str) -> PathBuf {
    dir.join(format!("{:016x}.img", fnv1a(url)))
}

/// Where `url` is (or would be) cached.
pub fn cache_path(url: &str) -> Option<PathBuf> {
    Some(cache_path_in(
        &lightline_dir()?.join("cache").join("images"),
        url,
    ))
}

/// The cached copy of `url`, if it was downloaded before.
pub fn cached(url: &str) -> Option<PathBuf> {
    cache_path(url).filter(|path| path.is_file())
}

/// Downloads `url` into the cache and returns the cached file. Only
/// `http`/`https` addresses are fetched, within a time and size limit.
pub fn download(url: &str) -> Result<PathBuf, String> {
    if !is_web(url) {
        return Err(format!("not a web address: {url}"));
    }
    let target = cache_path(url).ok_or("could not find the cache folder")?;
    let mut response = ureq::get(url)
        .config()
        .timeout_global(Some(TIMEOUT))
        .build()
        .call()
        .map_err(|error| format!("could not load {url}: {error}"))?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()
        .map_err(|error| format!("could not read {url}: {error}"))?;
    let parent = target.parent().ok_or("bad cache path")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    // Written beside the target and renamed, so a half-written file is
    // never mistaken for a cached image.
    let partial = target.with_extension(format!("{}.part", std::process::id()));
    fs::write(&partial, &bytes).map_err(|error| error.to_string())?;
    fs::rename(&partial, &target).map_err(|error| {
        let _ = fs::remove_file(&partial);
        error.to_string()
    })?;
    Ok(target)
}

/// True for SVG content, whatever the file is called: badge services serve
/// SVG from addresses with no extension.
pub fn is_svg(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_ascii_lowercase();
    let head = head.trim_start_matches('\u{feff}').trim_start();
    head.starts_with("<svg")
        || ((head.starts_with("<?xml")
            || head.starts_with("<!--")
            || head.starts_with("<!doctype"))
            && head.contains("<svg"))
}

fn allowed_file() -> Option<PathBuf> {
    Some(lightline_dir()?.join("markdown-web-images.txt"))
}

/// The folders whose Markdown previews may load web images.
pub fn allowed_folders() -> Vec<PathBuf> {
    allowed_file()
        .and_then(|file| fs::read_to_string(file).ok())
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Remembers that previews in `folder` may load web images.
pub fn allow_folder(folder: &Path) -> Result<(), String> {
    let file = allowed_file().ok_or("could not find the settings folder")?;
    let mut folders = allowed_folders();
    if folders.iter().any(|known| known == folder) {
        return Ok(());
    }
    folders.push(folder.to_path_buf());
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text: Vec<String> = folders
        .iter()
        .map(|folder| folder.to_string_lossy().into_owned())
        .collect();
    fs::write(file, text.join("\n")).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_names_are_stable_and_distinct() {
        let dir = Path::new("cache");
        let a = cache_path_in(dir, "https://img.shields.io/badge/a");
        assert_eq!(a, cache_path_in(dir, "https://img.shields.io/badge/a"));
        assert_ne!(a, cache_path_in(dir, "https://img.shields.io/badge/b"));
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
    }

    #[test]
    fn recognizes_svg_content() {
        assert!(is_svg(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"));
        assert!(is_svg(b"\xef\xbb\xbf<?xml version=\"1.0\"?>\n<svg></svg>"));
        assert!(is_svg(b"<!-- badge -->\n<svg></svg>"));
        assert!(!is_svg(b"\x89PNG\r\n\x1a\n"));
        assert!(!is_svg(b"<html><body>not an image</body></html>"));
    }

    #[test]
    fn only_web_addresses_are_downloaded() {
        assert!(download("file:///C:/Windows/win.ini").is_err());
        assert!(download("javascript:alert(1)").is_err());
    }
}
