//! Per-user Windows installation: file association, right-click menus, PATH and an
//! "Apps & features" entry. Everything lives under HKEY_CURRENT_USER, so no admin rights.

use std::path::{Path, PathBuf};

pub const APP_DIR_NAME: &str = "bin2";
pub const CLI_EXE: &str = "bin2.exe";
pub const SHELL_EXE: &str = "bin2w.exe";

const PROG_ID: &str = "bin2.file";
const CLASSES: &str = r"Software\Classes";
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\bin2";

/// Keys owned by bin2 under `Software\Classes`, removed whole on uninstall.
pub const OWNED_CLASS_KEYS: [&str; 3] = [PROG_ID, r"*\shell\bin2.seal", r"*\shell\bin2.sealhtml"];

/// One string value to write under `HKCU\<key>`. An empty name is the key's default value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegValue {
    pub key: String,
    pub name: &'static str,
    pub data: String,
}

fn value(key: impl Into<String>, name: &'static str, data: impl Into<String>) -> RegValue {
    RegValue {
        key: key.into(),
        name,
        data: data.into(),
    }
}

/// Where bin2 installs for the current user: `%LOCALAPPDATA%\Programs\bin2`.
pub fn install_dir(local_app_data: &Path) -> PathBuf {
    local_app_data.join("Programs").join(APP_DIR_NAME)
}

/// Every registry value bin2 writes, for executables installed in `dir`.
pub fn registry_values(dir: &Path, version: &str) -> Vec<RegValue> {
    let bin2w = dir.join(SHELL_EXE).display().to_string();
    let bin2 = dir.join(CLI_EXE).display().to_string();
    let icon = format!("\"{bin2w}\",0");
    let cmd = |args: &str| format!("\"{bin2w}\" {args} \"%1\"");
    let class = |sub: &str| format!(r"{CLASSES}\{sub}");

    vec![
        // .bin2 files: what they are, and that browsers should treat them as pages.
        value(class(".bin2"), "", PROG_ID),
        value(class(".bin2"), "Content Type", "text/html"),
        value(class(".bin2"), "PerceivedType", "document"),
        value(class(PROG_ID), "", "bin2 sealed file"),
        value(class(&format!(r"{PROG_ID}\DefaultIcon")), "", icon.clone()),
        value(class(&format!(r"{PROG_ID}\shell")), "", "open"),
        value(
            class(&format!(r"{PROG_ID}\shell\open")),
            "",
            "Open with bin2",
        ),
        value(
            class(&format!(r"{PROG_ID}\shell\open")),
            "Icon",
            icon.clone(),
        ),
        value(
            class(&format!(r"{PROG_ID}\shell\open\command")),
            "",
            cmd("open"),
        ),
        value(
            class(&format!(r"{PROG_ID}\shell\extract")),
            "",
            "Extract with bin2",
        ),
        value(
            class(&format!(r"{PROG_ID}\shell\extract")),
            "Icon",
            icon.clone(),
        ),
        value(
            class(&format!(r"{PROG_ID}\shell\extract\command")),
            "",
            cmd("extract"),
        ),
        // Right-click on any file.
        value(class(r"*\shell\bin2.seal"), "MUIVerb", "Seal with bin2"),
        value(class(r"*\shell\bin2.seal"), "Icon", icon.clone()),
        value(class(r"*\shell\bin2.seal\command"), "", cmd("seal")),
        value(
            class(r"*\shell\bin2.sealhtml"),
            "MUIVerb",
            "Seal with bin2 (shareable .html)",
        ),
        value(class(r"*\shell\bin2.sealhtml"), "Icon", icon),
        value(
            class(r"*\shell\bin2.sealhtml\command"),
            "",
            cmd("seal --html"),
        ),
        // Settings > Apps > Installed apps.
        value(UNINSTALL_KEY, "DisplayName", "bin2"),
        value(UNINSTALL_KEY, "DisplayVersion", version),
        value(UNINSTALL_KEY, "Publisher", "Dewmith Mihisara"),
        value(UNINSTALL_KEY, "DisplayIcon", format!("\"{bin2w}\",0")),
        value(UNINSTALL_KEY, "InstallLocation", dir.display().to_string()),
        value(
            UNINSTALL_KEY,
            "UninstallString",
            format!("\"{bin2}\" uninstall"),
        ),
    ]
}

fn same_dir(a: &str, b: &str) -> bool {
    a.trim_end_matches('\\')
        .eq_ignore_ascii_case(b.trim_end_matches('\\'))
}

/// `path` with `dir` appended, unless it is already there.
pub fn path_with(path: &str, dir: &str) -> String {
    if path.split(';').any(|p| same_dir(p, dir)) {
        return path.to_string();
    }
    let base = path.trim_end_matches(';');
    if base.is_empty() {
        dir.to_string()
    } else {
        format!("{base};{dir}")
    }
}

/// `path` with every entry equal to `dir` removed.
pub fn path_without(path: &str, dir: &str) -> String {
    path.split(';')
        .filter(|p| !same_dir(p, dir))
        .collect::<Vec<_>>()
        .join(";")
}

#[cfg(windows)]
pub use win::{install, uninstall};

#[cfg(not(windows))]
pub fn install() -> Result<(), String> {
    Err("bin2 install is only available on Windows for now".into())
}

#[cfg(not(windows))]
pub fn uninstall() -> Result<(), String> {
    Err("bin2 uninstall is only available on Windows for now".into())
}

#[cfg(windows)]
mod win {
    use std::fs;
    use std::io;
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };
    use winreg::enums::{RegType, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::{RegKey, RegValue as RawValue};

    use super::*;
    use crate::shell::win::wide;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn install() -> Result<(), String> {
        let dir = target_dir()?;
        let source = std::env::current_exe().map_err(|e| e.to_string())?;
        let source_dir = source
            .parent()
            .ok_or("cannot locate bin2.exe")?
            .to_path_buf();
        let shell_src = source_dir.join(SHELL_EXE);
        if !shell_src.is_file() {
            return Err(format!(
                "{SHELL_EXE} must sit next to {CLI_EXE} ({} is missing)",
                shell_src.display()
            ));
        }

        fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        if !same_path(&source_dir, &dir) {
            for (from, name) in [(&source, CLI_EXE), (&shell_src, SHELL_EXE)] {
                let to = dir.join(name);
                fs::copy(from, &to)
                    .map_err(|e| format!("could not copy to {}: {e}", to.display()))?;
            }
        }

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        for v in registry_values(&dir, env!("CARGO_PKG_VERSION")) {
            let (key, _) = hkcu
                .create_subkey(&v.key)
                .map_err(|e| format!(r"HKCU\{}: {e}", v.key))?;
            key.set_value(v.name, &v.data)
                .map_err(|e| format!(r"HKCU\{}: {e}", v.key))?;
        }

        let dir_str = dir.display().to_string();
        edit_user_path(|p| path_with(p, &dir_str))?;
        notify_shell();

        println!("bin2 is installed in {}", dir.display());
        println!("  - Right-click any file: \"Seal with bin2\" (Windows 11: under \"Show more options\")");
        println!("  - Double-click a .bin2 file to open it");
        println!("  - Open a new terminal to use the bin2 command anywhere");
        println!("  - Remove it any time with: bin2 uninstall");
        Ok(())
    }

    pub fn uninstall() -> Result<(), String> {
        let dir = target_dir()?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let classes = hkcu
            .open_subkey_with_flags(CLASSES, KEY_READ | KEY_WRITE)
            .map_err(|e| format!(r"HKCU\{CLASSES}: {e}"))?;

        // Only remove the .bin2 association if it still points at bin2.
        let ext_is_ours = classes
            .open_subkey(".bin2")
            .and_then(|k| k.get_value::<String, _>(""))
            .map(|v| v == PROG_ID)
            .unwrap_or(false);
        if ext_is_ours {
            delete_tree(&classes, ".bin2")?;
        }
        for key in OWNED_CLASS_KEYS {
            delete_tree(&classes, key)?;
        }
        delete_tree(&hkcu, UNINSTALL_KEY)?;

        let dir_str = dir.display().to_string();
        edit_user_path(|p| path_without(p, &dir_str))?;
        notify_shell();
        remove_files(&dir)?;

        println!(
            "bin2 is uninstalled. Your .bin2 files are untouched and still open in any browser."
        );
        Ok(())
    }

    fn target_dir() -> Result<PathBuf, String> {
        let base = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?;
        Ok(install_dir(Path::new(&base)))
    }

    fn same_path(a: &Path, b: &Path) -> bool {
        match (fs::canonicalize(a), fs::canonicalize(b)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    }

    fn delete_tree(parent: &RegKey, key: &str) -> Result<(), String> {
        match parent.delete_subkey_all(key) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("could not remove {key}: {e}")),
        }
    }

    /// Rewrite the user's PATH, keeping its registry type. Leaves it alone if it can't be read.
    fn edit_user_path(edit: impl Fn(&str) -> String) -> Result<(), String> {
        let env = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
            .map_err(|e| format!(r"HKCU\Environment: {e}"))?;
        let (current, vtype) = match env.get_raw_value("Path") {
            Ok(raw) => (decode_utf16(&raw.bytes), raw.vtype),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                (String::new(), RegType::REG_EXPAND_SZ)
            }
            Err(e) => return Err(format!("could not read your PATH, left unchanged: {e}")),
        };
        let updated = edit(&current);
        if updated == current {
            return Ok(());
        }
        let raw = RawValue {
            bytes: encode_utf16(&updated),
            vtype,
        };
        env.set_raw_value("Path", &raw)
            .map_err(|e| format!("could not update PATH: {e}"))?;

        let env_name = wide("Environment");
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                env_name.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                5000,
                std::ptr::null_mut(),
            );
        }
        Ok(())
    }

    fn decode_utf16(bytes: &[u8]) -> String {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
            .trim_end_matches('\0')
            .to_string()
    }

    fn encode_utf16(s: &str) -> Vec<u8> {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    /// Ask Explorer to reload icons and menus now instead of after a restart.
    fn notify_shell() {
        unsafe {
            SHChangeNotify(
                SHCNE_ASSOCCHANGED as _,
                SHCNF_IDLIST,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
    }

    /// Remove only the files bin2 installed, then the folder if it is empty.
    fn remove_files(dir: &Path) -> Result<(), String> {
        if !dir.exists() {
            return Ok(());
        }
        let running = std::env::current_exe().ok();
        let running_inside = running
            .as_deref()
            .and_then(Path::parent)
            .is_some_and(|p| same_path(p, dir));
        if running_inside {
            // Windows can't delete a running exe, so a hidden cmd finishes the job after we exit.
            let script = format!(
                "ping -n 3 127.0.0.1 >nul & del /f /q \"{0}\\{CLI_EXE}\" \"{0}\\{SHELL_EXE}\" & rmdir \"{0}\"",
                dir.display()
            );
            Command::new("cmd")
                .arg("/c")
                .raw_arg(script)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| format!("could not schedule removal of {}: {e}", dir.display()))?;
            return Ok(());
        }
        for name in [CLI_EXE, SHELL_EXE] {
            let file = dir.join(name);
            if file.exists() {
                fs::remove_file(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            }
        }
        let _ = fs::remove_dir(dir); // only succeeds when empty, which is what we want
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_registry_values() {
        let dir = Path::new(r"C:\Users\me\AppData\Local\Programs\bin2");
        let values = registry_values(dir, "0.1.0");
        let find = |key: &str, name: &str| {
            values
                .iter()
                .find(|v| v.key.ends_with(key) && v.name == name)
                .map(|v| v.data.as_str())
                .unwrap_or_else(|| panic!("missing {key} {name}"))
        };
        assert_eq!(find(r"Classes\.bin2", ""), "bin2.file");
        assert_eq!(find(r"Classes\.bin2", "Content Type"), "text/html");
        assert_eq!(
            find(r"*\shell\bin2.seal\command", ""),
            r#""C:\Users\me\AppData\Local\Programs\bin2\bin2w.exe" seal "%1""#
        );
        assert_eq!(
            find(r"*\shell\bin2.sealhtml\command", ""),
            r#""C:\Users\me\AppData\Local\Programs\bin2\bin2w.exe" seal --html "%1""#
        );
        assert_eq!(
            find(r"bin2.file\shell\open\command", ""),
            r#""C:\Users\me\AppData\Local\Programs\bin2\bin2w.exe" open "%1""#
        );
        assert_eq!(
            find(r"Uninstall\bin2", "UninstallString"),
            r#""C:\Users\me\AppData\Local\Programs\bin2\bin2.exe" uninstall"#
        );
    }

    #[test]
    fn every_value_is_removed_on_uninstall() {
        let values = registry_values(Path::new(r"C:\x"), "0.1.0");
        for v in &values {
            let rel = v.key.strip_prefix(r"Software\Classes\");
            let covered = match rel {
                Some(rel) => {
                    rel == ".bin2"
                        || OWNED_CLASS_KEYS
                            .iter()
                            .any(|k| rel == *k || rel.starts_with(&format!(r"{k}\")))
                }
                None => v.key == UNINSTALL_KEY,
            };
            assert!(covered, "{} is not removed by uninstall", v.key);
        }
    }

    #[test]
    fn edits_path_safely() {
        let dir = r"C:\Users\me\AppData\Local\Programs\bin2";
        assert_eq!(path_with("", dir), dir);
        assert_eq!(path_with(r"C:\a;", dir), format!(r"C:\a;{dir}"));
        assert_eq!(
            path_with(&format!(r"C:\a;{}\", dir.to_uppercase()), dir),
            format!(r"C:\a;{}\", dir.to_uppercase())
        );
        assert_eq!(
            path_without(&format!(r"C:\a;{dir};%USERPROFILE%\b"), dir),
            r"C:\a;%USERPROFILE%\b"
        );
        assert_eq!(path_without(r"C:\a;C:\b", dir), r"C:\a;C:\b");
    }

    #[test]
    fn installs_under_local_programs() {
        assert_eq!(
            install_dir(Path::new(r"C:\Users\me\AppData\Local")),
            Path::new(r"C:\Users\me\AppData\Local\Programs\bin2")
        );
    }
}
