//! `bin2w`: the windowless helper Explorer runs for right-click and double-click actions.
//!
//! It never opens a console window. It stays silent on success, like WinRAR,
//! and shows a message box when something goes wrong.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use bin2_cli::shell::{message_box, open_in_browser};
use bin2_cli::{extract_file, seal_file, Collision};

const USAGE: &str = "bin2w is used by Windows Explorer.\n\n\
    bin2w seal [--html] <file>...\n\
    bin2w extract <file>...\n\
    bin2w open <file>";

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            message_box("bin2", &msg, true);
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[OsString]) -> Result<(), String> {
    let Some((command, rest)) = args.split_first() else {
        return Err(USAGE.into());
    };
    let html = rest.iter().any(|a| a == "--html");
    let files: Vec<PathBuf> = rest
        .iter()
        .filter(|a| *a != "--html")
        .map(PathBuf::from)
        .collect();
    if files.is_empty() {
        return Err(USAGE.into());
    }

    match command.to_string_lossy().as_ref() {
        "seal" => for_each(&files, "seal", |f| {
            seal_file(f, None, Collision::Rename, html).map(|_| ())
        }),
        "extract" => for_each(&files, "extract", |f| {
            extract_file(f, None, Collision::Rename).map(|_| ())
        }),
        "open" => files.iter().try_for_each(|f| open_in_browser(f)),
        _ => Err(USAGE.into()),
    }
}

/// Run `action` on every file, then report all failures together.
fn for_each(
    files: &[PathBuf],
    verb: &str,
    action: impl Fn(&std::path::Path) -> Result<(), String>,
) -> Result<(), String> {
    let failures: Vec<String> = files
        .iter()
        .filter_map(|f| {
            action(f).err().map(|e| {
                let name = f.file_name().unwrap_or(f.as_os_str()).to_string_lossy();
                format!("{name}: {e}")
            })
        })
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("Could not {verb}:\n\n{}", failures.join("\n")))
    }
}
