//! bin2 core: seal any file into a self-contained `.bin2` page, and get it back out.
//!
//! A `.bin2` file is a single HTML document that carries the original bytes,
//! a manifest, a static fallback, and its own viewer. It opens in the bin2 app,
//! or in any web browser if bin2 is not installed.

pub mod detect;
mod html;
mod time;

use std::fmt;
use std::io::{Read, Write};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use detect::{detect, Detected, Kind};

/// Version of the container format written into every manifest.
pub const FORMAT_VERSION: u32 = 1;
/// File extension for sealed files, without the dot.
pub const EXTENSION: &str = "bin2";

const SHELL: &str = include_str!("../../../template/shell.html");
const VIEWER: &str = include_str!("../../../template/viewer.js");

const MANIFEST_OPEN: &str = r#"<script type="application/json" id="bin2-manifest">"#;
const PAYLOAD_SCRIPT_OPEN: &str = r#"<script type="application/octet-stream" id="bin2-payload">"#;
const PAYLOAD_IMG_OPEN: &str = r#"<img id="bin2-payload" class="static-img" alt="" src="data:"#;
const SCRIPT_CLOSE: &str = "</script>";

/// Text files larger than this are not duplicated into the no-JavaScript fallback.
const TEXT_FALLBACK_LIMIT: usize = 1024 * 1024;
/// Base64 line length inside the payload, so text editors can still open the file.
const B64_LINE: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Encoding {
    #[serde(rename = "base64")]
    Base64,
    #[serde(rename = "gzip+base64")]
    GzipBase64,
}

/// Everything bin2 records about the original file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub size: u64,
    pub mime: String,
    pub kind: Kind,
    pub sha256: String,
    pub encoding: Encoding,
    pub sealed_at: String,
    pub tool: String,
}

#[derive(Debug)]
pub enum Error {
    NotBin2(&'static str),
    BadManifest(serde_json::Error),
    BadPayload(String),
    HashMismatch { expected: String, actual: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotBin2(why) => write!(f, "not a bin2 file ({why})"),
            Error::BadManifest(e) => write!(f, "the manifest is damaged: {e}"),
            Error::BadPayload(e) => write!(f, "the stored file is damaged: {e}"),
            Error::HashMismatch { expected, actual } => {
                write!(
                    f,
                    "integrity check failed: expected SHA-256 {expected}, got {actual}"
                )
            }
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// Options for [`seal_with`].
#[derive(Debug, Clone, Default)]
pub struct SealOptions {
    /// Unix timestamp to record as the sealing time. Defaults to now.
    pub sealed_at_unix: Option<i64>,
}

/// Seal `bytes` (originally named `name`) into a `.bin2` document.
pub fn seal(name: &str, bytes: &[u8]) -> String {
    seal_with(name, bytes, &SealOptions::default())
}

/// Seal with explicit options. Output is deterministic for the same inputs and options.
pub fn seal_with(name: &str, bytes: &[u8], opts: &SealOptions) -> String {
    let name = clean_name(name);
    let found = detect(&name, bytes);
    let sha256 = hex(&Sha256::digest(bytes));

    // Images are stored as a plain data URI so they show even without JavaScript.
    // Everything else is gzipped when that actually saves space.
    let (encoding, stored) = if found.kind == Kind::Image {
        (Encoding::Base64, bytes.to_vec())
    } else {
        let gz = gzip(bytes);
        if gz.len() < bytes.len() * 95 / 100 {
            (Encoding::GzipBase64, gz)
        } else {
            (Encoding::Base64, bytes.to_vec())
        }
    };
    let b64 = B64.encode(&stored);

    let manifest = Manifest {
        format: "bin2".into(),
        version: FORMAT_VERSION,
        name: name.clone(),
        size: bytes.len() as u64,
        mime: found.mime.into(),
        kind: found.kind,
        sha256,
        encoding,
        sealed_at: time::iso8601(opts.sealed_at_unix.unwrap_or_else(time::now_unix)),
        tool: format!("bin2 {}", env!("CARGO_PKG_VERSION")),
    };

    let payload = if found.kind == Kind::Image {
        "<!-- payload: stored in the image above -->".to_string()
    } else {
        format!(
            "{PAYLOAD_SCRIPT_OPEN}\n{}\n{SCRIPT_CLOSE}",
            wrap(&b64, B64_LINE)
        )
    };
    let static_part = match found.kind {
        Kind::Image => format!("{PAYLOAD_IMG_OPEN}{};base64,{b64}\">", found.mime),
        _ => static_fallback(found.kind, bytes),
    };

    html::render(
        SHELL,
        &[
            ("TITLE", &html::escape(&name)),
            (
                "SUBTITLE",
                &html::escape(&format!(
                    "{} · {}",
                    human_size(bytes.len() as u64),
                    found.mime
                )),
            ),
            ("STATIC", &static_part),
            ("MANIFEST", &manifest_json(&manifest)),
            ("PAYLOAD", &payload),
            ("VIEWER", VIEWER),
        ],
    )
}

/// Read the manifest of a `.bin2` document without decoding the payload.
pub fn read_manifest(doc: &str) -> Result<Manifest> {
    let start = doc
        .find(MANIFEST_OPEN)
        .ok_or(Error::NotBin2("no manifest found"))?
        + MANIFEST_OPEN.len();
    let len = doc[start..]
        .find(SCRIPT_CLOSE)
        .ok_or(Error::NotBin2("manifest is not closed"))?;
    let manifest: Manifest =
        serde_json::from_str(&doc[start..start + len]).map_err(Error::BadManifest)?;
    if manifest.format != "bin2" {
        return Err(Error::NotBin2("manifest format is not bin2"));
    }
    Ok(manifest)
}

/// Recover the original file from a `.bin2` document, verifying its SHA-256.
pub fn unseal(doc: &str) -> Result<(Manifest, Vec<u8>)> {
    let manifest = read_manifest(doc)?;
    let b64 = payload_text(doc)?;
    let stored = B64
        .decode(
            b64.bytes()
                .filter(|c| !c.is_ascii_whitespace())
                .collect::<Vec<u8>>(),
        )
        .map_err(|e| Error::BadPayload(e.to_string()))?;
    let bytes = match manifest.encoding {
        Encoding::Base64 => stored,
        Encoding::GzipBase64 => {
            let mut out = Vec::with_capacity(manifest.size as usize);
            GzDecoder::new(&stored[..])
                .read_to_end(&mut out)
                .map_err(|e| Error::BadPayload(e.to_string()))?;
            out
        }
    };
    let actual = hex(&Sha256::digest(&bytes));
    if actual != manifest.sha256 {
        return Err(Error::HashMismatch {
            expected: manifest.sha256,
            actual,
        });
    }
    Ok((manifest, bytes))
}

fn payload_text(doc: &str) -> Result<&str> {
    if let Some(i) = doc.find(PAYLOAD_SCRIPT_OPEN) {
        let start = i + PAYLOAD_SCRIPT_OPEN.len();
        let len = doc[start..]
            .find(SCRIPT_CLOSE)
            .ok_or(Error::NotBin2("payload is not closed"))?;
        return Ok(&doc[start..start + len]);
    }
    if let Some(i) = doc.find(PAYLOAD_IMG_OPEN) {
        let rest = &doc[i + PAYLOAD_IMG_OPEN.len()..];
        let comma = rest
            .find(";base64,")
            .ok_or(Error::NotBin2("image payload is not base64"))?
            + ";base64,".len();
        let len = rest[comma..]
            .find('"')
            .ok_or(Error::NotBin2("image payload is not closed"))?;
        return Ok(&rest[comma..comma + len]);
    }
    Err(Error::NotBin2("no payload found"))
}

/// What a user sees with JavaScript turned off.
fn static_fallback(kind: Kind, bytes: &[u8]) -> String {
    let text_kind = matches!(kind, Kind::Text | Kind::Markdown | Kind::Html);
    if text_kind && bytes.len() <= TEXT_FALLBACK_LIMIT {
        if let Ok(text) = std::str::from_utf8(bytes) {
            return format!(
                r#"<div class="doc"><pre class="text">{}</pre></div>"#,
                html::escape(text)
            );
        }
    }
    concat!(
        r#"<div class="center msg"><noscript><h2>Turn on JavaScript to view this file</h2>"#,
        r#"<p>Or open it with the bin2 app. The original file is stored safely inside, "#,
        r#"and the note at the top of this file explains how to recover it by hand.</p></noscript></div>"#
    )
    .to_string()
}

/// Strip any directory parts, keeping only a safe file name.
pub fn clean_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.').to_string();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        "file".to_string()
    } else {
        trimmed
    }
}

fn manifest_json(m: &Manifest) -> String {
    // `<` never appears in JSON outside strings, and `\u003c` is a valid escape inside them,
    // so this keeps `</script>` from ever appearing in the manifest.
    serde_json::to_string_pretty(m)
        .expect("manifest serializes")
        .replace('<', "\\u003c")
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::best());
    enc.write_all(bytes).expect("writing to memory cannot fail");
    enc.finish().expect("writing to memory cannot fail")
}

fn wrap(s: &str, width: usize) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / width + 1);
    for (i, chunk) in s.as_bytes().chunks(width).enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(std::str::from_utf8(chunk).expect("base64 is ASCII"));
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Human-readable size, matching the viewer's formatting.
pub fn human_size(n: u64) -> String {
    let units = ["bytes", "KB", "MB", "GB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} {}", units[0])
    } else {
        format!("{v:.1} {}", units[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed() -> SealOptions {
        SealOptions {
            sealed_at_unix: Some(1_761_134_400),
        }
    }

    fn noise(len: usize) -> Vec<u8> {
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 32) as u8
            })
            .collect()
    }

    fn roundtrip(name: &str, bytes: &[u8]) -> Manifest {
        let doc = seal_with(name, bytes, &fixed());
        let (m, back) = unseal(&doc).expect("unseals");
        assert_eq!(back, bytes, "bytes survive for {name}");
        m
    }

    #[test]
    fn roundtrips_every_kind() {
        let png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR".to_vec();
        assert_eq!(roundtrip("a.png", &png).kind, Kind::Image);
        assert_eq!(roundtrip("doc.pdf", b"%PDF-1.4\n%%EOF").kind, Kind::Pdf);
        assert_eq!(
            roundtrip("notes.md", "# Hello\n\nworld ✓".as_bytes()).kind,
            Kind::Markdown
        );
        assert_eq!(roundtrip("empty.txt", b"").size, 0);
        let noise = noise(5000);
        assert_eq!(roundtrip("noise.dat", &noise).kind, Kind::Binary);
    }

    #[test]
    fn compresses_text_but_not_noise() {
        let text = "the same line again\n".repeat(500);
        let m = roundtrip("log.txt", text.as_bytes());
        assert_eq!(m.encoding, Encoding::GzipBase64);
        let noise = noise(5000);
        assert_eq!(roundtrip("noise.dat", &noise).encoding, Encoding::Base64);
    }

    #[test]
    fn hostile_content_cannot_break_the_page() {
        let evil_name = r#"</script><script>alert(1)</script>".md"#;
        let evil_body = b"</script><script>alert(2)</script>\n<!-- id=\"bin2-payload\" -->";
        let doc = seal_with(evil_name, evil_body, &fixed());
        assert!(!doc.contains("<script>alert"), "no injected script tags");
        let (m, back) = unseal(&doc).unwrap();
        assert_eq!(back, evil_body);
        assert!(!m.name.contains('/') && !m.name.contains('<'));
    }

    #[test]
    fn detects_tampering() {
        let doc = seal_with("a.txt", b"original", &fixed());
        let m = read_manifest(&doc).unwrap();
        let tampered = doc.replace(&m.sha256, &"0".repeat(64));
        assert!(matches!(unseal(&tampered), Err(Error::HashMismatch { .. })));
        assert!(matches!(unseal("<html>hi</html>"), Err(Error::NotBin2(_))));
    }

    #[test]
    fn is_deterministic() {
        assert_eq!(
            seal_with("a.txt", b"x", &fixed()),
            seal_with("a.txt", b"x", &fixed())
        );
    }

    #[test]
    fn starts_with_a_human_notice() {
        let doc = seal_with("a.txt", b"x", &fixed());
        assert!(doc.starts_with("<!-- This is a bin2 file."));
        assert!(doc.contains("2025-10-22T12:00:00Z"));
    }

    #[test]
    fn cleans_names() {
        assert_eq!(clean_name(r"C:\Users\me\photo.jpg"), "photo.jpg");
        assert_eq!(clean_name("../../etc/passwd"), "passwd");
        assert_eq!(clean_name(".."), "file");
        assert_eq!(clean_name("a<b>.txt"), "a_b_.txt");
    }
}
