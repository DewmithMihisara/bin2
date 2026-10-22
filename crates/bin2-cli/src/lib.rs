//! Actions shared by the `bin2` command-line tool and the Windows shell helper.

pub mod install;
pub mod shell;

use std::fs;
use std::path::{Path, PathBuf};

use bin2_core::Manifest;

/// What to do when the output file already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collision {
    /// Stop with an error.
    Fail,
    /// Replace the existing file.
    Overwrite,
    /// Pick a free name like `photo (2).jpg`, as Explorer does.
    Rename,
}

/// Seal one file. Returns the path of the written `.bin2` file.
pub fn seal_file(
    path: &Path,
    out_dir: Option<&Path>,
    collision: Collision,
    html: bool,
) -> Result<PathBuf, String> {
    if path.is_dir() {
        return Err("is a folder (folder sealing is coming in v0.2)".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let doc = bin2_core::seal(&name, &bytes);

    let dir = out_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| parent_of(path));
    let suffix = if html { ".html" } else { "" };
    let out = dir.join(format!(
        "{}.{}{suffix}",
        bin2_core::clean_name(&name),
        bin2_core::EXTENSION
    ));
    write_out(&out, doc.as_bytes(), collision)
}

/// Recover the original from a `.bin2` file. Returns its manifest and where it was written.
pub fn extract_file(
    file: &Path,
    out_dir: Option<&Path>,
    collision: Collision,
) -> Result<(Manifest, PathBuf), String> {
    let doc = read_doc(file)?;
    let (manifest, bytes) =
        bin2_core::unseal(&doc).map_err(|e| format!("{}: {e}", file.display()))?;
    let dir = out_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| parent_of(file));
    let out = dir.join(bin2_core::clean_name(&manifest.name));
    let written = write_out(&out, &bytes, collision)?;
    Ok((manifest, written))
}

/// Read a `.bin2` file as text.
pub fn read_doc(file: &Path) -> Result<String, String> {
    let bytes = fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    String::from_utf8(bytes).map_err(|_| format!("{}: not a bin2 file (not text)", file.display()))
}

/// The folder a path lives in, or `.` for a bare file name.
pub fn parent_of(path: &Path) -> PathBuf {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Write `bytes` to `out`, honouring the collision rule. Returns the path actually written.
pub fn write_out(out: &Path, bytes: &[u8], collision: Collision) -> Result<PathBuf, String> {
    let out = &match collision {
        Collision::Fail if out.exists() => {
            return Err(format!(
                "{} already exists (use --force to overwrite)",
                out.display()
            ))
        }
        Collision::Rename => unique_path(out),
        _ => out.to_path_buf(),
    };
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::write(out, bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(out.to_path_buf())
}

/// Suffixes kept together when numbering a name, longest first.
const COMPOUND_SUFFIXES: [&str; 2] = [".bin2.html", ".bin2"];

/// Return `path` if it is free, otherwise the first free `name (2).ext`, `name (3).ext`, ...
pub fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, suffix) = split_suffix(&name);
    let dir = parent_of(path);
    (2u32..)
        .map(|n| dir.join(format!("{stem} ({n}){suffix}")))
        .find(|candidate| !candidate.exists())
        .expect("some numbered name is free")
}

/// Split `photo.jpg.bin2` into (`photo.jpg`, `.bin2`) and `photo.jpg` into (`photo`, `.jpg`).
fn split_suffix(name: &str) -> (&str, &str) {
    let lower = name.to_ascii_lowercase();
    for suffix in COMPOUND_SUFFIXES {
        if lower.ends_with(suffix) && lower.len() > suffix.len() {
            return name.split_at(name.len() - suffix.len());
        }
    }
    match name.rfind(".") {
        Some(i) if i > 0 => name.split_at(i),
        _ => (name, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_suffixes() {
        assert_eq!(split_suffix("photo.jpg.bin2"), ("photo.jpg", ".bin2"));
        assert_eq!(
            split_suffix("photo.jpg.BIN2.html"),
            ("photo.jpg", ".BIN2.html")
        );
        assert_eq!(split_suffix("photo.jpg"), ("photo", ".jpg"));
        assert_eq!(split_suffix(".bashrc"), (".bashrc", ""));
        assert_eq!(split_suffix("README"), ("README", ""));
    }

    #[test]
    fn numbers_taken_names() {
        let dir = std::env::temp_dir().join(format!("bin2-unique-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let first = dir.join("a.txt.bin2");
        assert_eq!(unique_path(&first), first);

        write_out(&first, b"1", Collision::Fail).unwrap();
        let second = write_out(&first, b"2", Collision::Rename).unwrap();
        let third = write_out(&first, b"3", Collision::Rename).unwrap();
        assert_eq!(second, dir.join("a.txt (2).bin2"));
        assert_eq!(third, dir.join("a.txt (3).bin2"));
        assert!(write_out(&first, b"4", Collision::Fail).is_err());
        assert_eq!(fs::read(&first).unwrap(), b"1");

        fs::remove_dir_all(&dir).unwrap();
    }
}
