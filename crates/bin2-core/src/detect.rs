//! Figure out what a file is, from its bytes first and its name second.

use serde::{Deserialize, Serialize};

/// Which viewer a sealed file uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Shown directly by the browser as an `<img>`.
    Image,
    Pdf,
    Video,
    Audio,
    Text,
    Markdown,
    Html,
    /// No viewer yet. The original is still stored losslessly.
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detected {
    pub mime: &'static str,
    pub kind: Kind,
}

const fn d(mime: &'static str, kind: Kind) -> Detected {
    Detected { mime, kind }
}

fn extension(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// Detect the type of a file.
pub fn detect(name: &str, bytes: &[u8]) -> Detected {
    let ext = extension(name);
    if let Some(found) = by_magic(&ext, bytes) {
        return found;
    }
    if let Some(found) = as_text(&ext, bytes) {
        return found;
    }
    by_extension(&ext)
}

fn by_magic(ext: &str, b: &[u8]) -> Option<Detected> {
    let starts = |sig: &[u8]| b.starts_with(sig);
    let at = |offset: usize, sig: &[u8]| {
        b.len() >= offset + sig.len() && &b[offset..offset + sig.len()] == sig
    };

    if starts(b"\x89PNG\r\n\x1a\n") {
        return Some(d("image/png", Kind::Image));
    }
    if starts(b"\xFF\xD8\xFF") {
        return Some(d("image/jpeg", Kind::Image));
    }
    if starts(b"GIF87a") || starts(b"GIF89a") {
        return Some(d("image/gif", Kind::Image));
    }
    if starts(b"RIFF") && at(8, b"WEBP") {
        return Some(d("image/webp", Kind::Image));
    }
    if starts(b"RIFF") && at(8, b"WAVE") {
        return Some(d("audio/wav", Kind::Audio));
    }
    if starts(b"RIFF") && at(8, b"AVI ") {
        return Some(d("video/x-msvideo", Kind::Binary));
    }
    if starts(b"BM") && ext == "bmp" {
        return Some(d("image/bmp", Kind::Image));
    }
    if starts(b"\x00\x00\x01\x00") && ext == "ico" {
        return Some(d("image/x-icon", Kind::Image));
    }
    if starts(b"II*\x00") || starts(b"MM\x00*") {
        // Many camera RAW formats are TIFF containers underneath.
        return Some(if is_raw_extension(ext) {
            d("image/x-raw", Kind::Binary)
        } else {
            d("image/tiff", Kind::Binary)
        });
    }
    if starts(b"8BPS") {
        return Some(d("image/vnd.adobe.photoshop", Kind::Binary));
    }
    if starts(b"%PDF-") {
        return Some(d("application/pdf", Kind::Pdf));
    }
    if at(4, b"ftyp") && b.len() >= 12 {
        return Some(match &b[8..12] {
            b"heic" | b"heix" | b"hevc" | b"hevx" | b"mif1" | b"msf1" => {
                d("image/heic", Kind::Binary)
            }
            b"avif" | b"avis" => d("image/avif", Kind::Image),
            b"M4A " | b"M4B " => d("audio/mp4", Kind::Audio),
            b"qt  " => d("video/quicktime", Kind::Video),
            _ => d("video/mp4", Kind::Video),
        });
    }
    if starts(b"\x1A\x45\xDF\xA3") {
        return Some(if ext == "mkv" {
            d("video/x-matroska", Kind::Video)
        } else {
            d("video/webm", Kind::Video)
        });
    }
    if starts(b"ID3") || (b.len() >= 2 && b[0] == 0xFF && (b[1] & 0xE0) == 0xE0 && ext == "mp3") {
        return Some(d("audio/mpeg", Kind::Audio));
    }
    if starts(b"OggS") {
        return Some(d("audio/ogg", Kind::Audio));
    }
    if starts(b"fLaC") {
        return Some(d("audio/flac", Kind::Audio));
    }
    if starts(b"PK\x03\x04") || starts(b"PK\x05\x06") {
        return Some(match ext {
            "docx" => d(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                Kind::Binary,
            ),
            "xlsx" => d(
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                Kind::Binary,
            ),
            "pptx" => d(
                "application/vnd.openxmlformats-officedocument.presentationml.presentation",
                Kind::Binary,
            ),
            "odt" => d("application/vnd.oasis.opendocument.text", Kind::Binary),
            "epub" => d("application/epub+zip", Kind::Binary),
            _ => d("application/zip", Kind::Binary),
        });
    }
    None
}

fn as_text(ext: &str, b: &[u8]) -> Option<Detected> {
    let body = b.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(b);
    let text = std::str::from_utf8(body).ok()?;
    if text.contains('\0') {
        return None;
    }
    let head = text.trim_start();
    let head_lower: String = head
        .chars()
        .take(256)
        .collect::<String>()
        .to_ascii_lowercase();

    Some(match ext {
        "md" | "markdown" | "mdown" => d("text/markdown", Kind::Markdown),
        "html" | "htm" | "xhtml" => d("text/html", Kind::Html),
        "svg" => d("image/svg+xml", Kind::Image),
        "json" => d("application/json", Kind::Text),
        "csv" => d("text/csv", Kind::Text),
        _ if head_lower.starts_with("<svg")
            || (head_lower.starts_with("<?xml") && head_lower.contains("<svg")) =>
        {
            d("image/svg+xml", Kind::Image)
        }
        _ if head_lower.starts_with("<!doctype html") || head_lower.starts_with("<html") => {
            d("text/html", Kind::Html)
        }
        _ => d("text/plain", Kind::Text),
    })
}

fn is_raw_extension(ext: &str) -> bool {
    matches!(
        ext,
        "cr2" | "cr3" | "nef" | "arw" | "dng" | "orf" | "rw2" | "raf"
    )
}

fn by_extension(ext: &str) -> Detected {
    match ext {
        "heic" | "heif" => d("image/heic", Kind::Binary),
        _ if is_raw_extension(ext) => d("image/x-raw", Kind::Binary),
        "psd" => d("image/vnd.adobe.photoshop", Kind::Binary),
        "doc" => d("application/msword", Kind::Binary),
        "wpd" => d("application/vnd.wordperfect", Kind::Binary),
        _ => d("application/octet-stream", Kind::Binary),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_by_magic_not_name() {
        assert_eq!(
            detect("photo.txt", b"\x89PNG\r\n\x1a\n....").kind,
            Kind::Image
        );
        assert_eq!(detect("x", b"%PDF-1.7\n").mime, "application/pdf");
        assert_eq!(detect("clip", b"\0\0\0\x18ftypisom\0\0").mime, "video/mp4");
        assert_eq!(detect("img", b"\0\0\0\x18ftypheic\0\0").mime, "image/heic");
    }

    #[test]
    fn detects_text_flavours() {
        assert_eq!(detect("notes.md", b"# Hi").kind, Kind::Markdown);
        assert_eq!(detect("page", b"<!DOCTYPE html><p>").kind, Kind::Html);
        assert_eq!(
            detect("logo", b"<svg xmlns='http://www.w3.org/2000/svg'/>").kind,
            Kind::Image
        );
        assert_eq!(detect("a.log", b"hello").mime, "text/plain");
        assert_eq!(detect("empty", b"").kind, Kind::Text);
    }

    #[test]
    fn falls_back_to_binary() {
        assert_eq!(detect("blob", &[0, 159, 146, 150, 0]).kind, Kind::Binary);
        assert_eq!(detect("IMG_1.CR2", &[0, 1, 2, 255]).mime, "image/x-raw");
        assert_eq!(detect("IMG_2.CR2", b"II*\x00\x10\x00").mime, "image/x-raw");
        assert_eq!(detect("scan.tif", b"II*\x00\x10\x00").mime, "image/tiff");
        assert_eq!(detect("report.docx", b"PK\x03\x04rest").kind, Kind::Binary);
    }
}
