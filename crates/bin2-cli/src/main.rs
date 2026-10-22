//! `bin2`: seal files so they open anywhere, forever.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bin2_cli::{extract_file, read_doc, seal_file, Collision};
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
    let collision = collision(force);
    let mut failed = 0;
    for path in files {
        match seal_file(path, out_dir, collision, html) {
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

fn extract(file: &Path, out_dir: Option<&Path>, force: bool) -> Result<(), String> {
    let (_, out) = extract_file(file, out_dir, collision(force))?;
    println!(
        "extracted  {}  ->  {}  (SHA-256 verified)",
        file.display(),
        out.display()
    );
    Ok(())
}

fn collision(force: bool) -> Collision {
    if force {
        Collision::Overwrite
    } else {
        Collision::Fail
    }
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
