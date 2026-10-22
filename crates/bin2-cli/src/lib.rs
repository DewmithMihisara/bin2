//! Actions shared by the `bin2` command-line tool and the Windows shell helper.

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
    if out.exists() && collision == Collision::Fail {
        return Err(format!(
            "{} already exists (use --force to overwrite)",
            out.display()
        ));
    }
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::write(out, bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(out.to_path_buf())
}
