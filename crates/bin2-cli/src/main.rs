//! `bin2`: seal files so they open anywhere, forever.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "bin2",
    version,
    about = "Seal any file so it opens anywhere, forever."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Seal files into .bin2 files that open in bin2 or any web browser.
    Seal {
        /// Files to seal.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Folder to write the .bin2 files into. Defaults to next to each original.
        #[arg(short, long)]
        out_dir: Option<PathBuf>,
        /// Overwrite existing .bin2 files.
        #[arg(short, long)]
        force: bool,
        /// Write name.bin2.html instead, for sharing with people who do not have bin2.
        #[arg(long)]
        html: bool,
    },
    /// Recover the original file from a .bin2 file, checking its integrity.
    Extract {
        /// The .bin2 file.
        file: PathBuf,
        /// Folder to write the original into. Defaults to next to the .bin2 file.
        #[arg(short, long)]
        out_dir: Option<PathBuf>,
        /// Overwrite an existing file with the same name.
        #[arg(short, long)]
        force: bool,
    },
    /// Show what is inside a .bin2 file.
    Info {
        /// The .bin2 file.
        file: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Seal {
            files,
            out_dir,
            force,
            html,
        } => seal(&files, out_dir.as_deref(), force, html),
        Command::Extract {
            file,
            out_dir,
            force,
        } => extract(&file, out_dir.as_deref(), force),
        Command::Info { file } => info(&file),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("bin2: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn seal(files: &[PathBuf], out_dir: Option<&Path>, force: bool, html: bool) -> Result<(), String> {
    let mut failed = 0;
    for path in files {
        match seal_one(path, out_dir, force, html) {
            Ok(out) => println!("sealed  {}  ->  {}", path.display(), out.display()),
            Err(msg) => {
                eprintln!("bin2: {}: {msg}", path.display());
                failed += 1;
            }
        }
    }
    match failed {
        0 => Ok(()),
        n => Err(format!(
            "{n} of {} file(s) could not be sealed",
            files.len()
        )),
    }
}

fn seal_one(
    path: &Path,
    out_dir: Option<&Path>,
    force: bool,
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
    write_new(&out, doc.as_bytes(), force)?;
    Ok(out)
}

fn extract(file: &Path, out_dir: Option<&Path>, force: bool) -> Result<(), String> {
    let doc = read_doc(file)?;
    let (manifest, bytes) =
        bin2_core::unseal(&doc).map_err(|e| format!("{}: {e}", file.display()))?;
    let dir = out_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| parent_of(file));
    let out = dir.join(bin2_core::clean_name(&manifest.name));
    write_new(&out, &bytes, force)?;
    println!(
        "extracted  {}  ->  {}  (SHA-256 verified)",
        file.display(),
        out.display()
    );
    Ok(())
}

fn info(file: &Path) -> Result<(), String> {
    let doc = read_doc(file)?;
    let m = bin2_core::read_manifest(&doc).map_err(|e| format!("{}: {e}", file.display()))?;
    let integrity = match bin2_core::unseal(&doc) {
        Ok(_) => "verified".to_string(),
        Err(e) => format!("FAILED: {e}"),
    };
    let rows = [
        ("Name", m.name.clone()),
        ("Type", m.mime.clone()),
        ("Viewer", format!("{:?}", m.kind).to_lowercase()),
        ("Size", bin2_core::human_size(m.size)),
        ("Sealed", m.sealed_at.clone()),
        ("SHA-256", m.sha256.clone()),
        ("Encoding", serde_name(m.encoding)),
        ("Made with", m.tool.clone()),
        ("Integrity", integrity),
    ];
    for (k, v) in rows {
        println!("{k:<10} {v}");
    }
    Ok(())
}

fn serde_name(e: bin2_core::Encoding) -> String {
    match e {
        bin2_core::Encoding::Base64 => "base64".into(),
        bin2_core::Encoding::GzipBase64 => "gzip+base64".into(),
    }
}

fn read_doc(file: &Path) -> Result<String, String> {
    let bytes = fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    String::from_utf8(bytes).map_err(|_| format!("{}: not a bin2 file (not text)", file.display()))
}

fn parent_of(path: &Path) -> PathBuf {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

fn write_new(out: &Path, bytes: &[u8], force: bool) -> Result<(), String> {
    if out.exists() && !force {
        return Err(format!(
            "{} already exists (use --force to overwrite)",
            out.display()
        ));
    }
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::write(out, bytes).map_err(|e| format!("{}: {e}", out.display()))
}
