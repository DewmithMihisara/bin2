//! Desktop integration: message boxes and opening `.bin2` files in the default browser.

use std::path::Path;

/// Browser families that render a local `.bin2` as a page once its content type is registered.
const CHROMIUM_PROGIDS: [&str; 6] = ["MSEdge", "Chrome", "Brave", "Vivaldi", "Opera", "Chromium"];

/// Percent-encode an absolute local path as a `file://` URL.
pub fn file_url(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let (mut out, rest) = match s.strip_prefix("//") {
        Some(unc) => (String::from("file://"), unc.to_string()),
        None => (
            String::from("file:///"),
            s.trim_start_matches('/').to_string(),
        ),
    };
    for b in rest.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Split a registry command line like `"C:\x\app.exe" --flag %1` into program and arguments.
pub fn split_command(cmd: &str) -> Option<(String, String)> {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return None;
    }
    if let Some(rest) = cmd.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some((rest[..end].to_string(), rest[end + 1..].trim().to_string()));
    }
    Some(match cmd.split_once(' ') {
        Some((prog, args)) => (prog.to_string(), args.trim().to_string()),
        None => (cmd.to_string(), String::new()),
    })
}

/// Put `target` where the command template expects the file, or append it.
pub fn fill_args(template: &str, target: &str) -> String {
    if template.contains("%1") {
        template.replace("%1", target)
    } else {
        format!("{template} {target}").trim().to_string()
    }
}

pub fn is_chromium(progid: &str) -> bool {
    CHROMIUM_PROGIDS.iter().any(|p| progid.starts_with(p))
}

/// Open a `.bin2` file as a page in the user's default browser.
pub fn open_in_browser(path: &Path) -> Result<(), String> {
    let abs = std::path::absolute(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !abs.is_file() {
        return Err(format!("{} does not exist", abs.display()));
    }
    #[cfg(windows)]
    {
        if win::open_with_default_browser(&abs) {
            return Ok(());
        }
        // Other browsers may not honour the registered content type, so give them a real .html copy.
        let copy = temp_html_copy(&abs)?;
        win::shell_open(&copy)
    }
    #[cfg(not(windows))]
    {
        let _ = abs;
        Err("opening files is only supported on Windows for now".into())
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn temp_html_copy(abs: &Path) -> Result<std::path::PathBuf, String> {
    let dir = std::env::temp_dir().join("bin2");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let name = abs
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = bin2_core::clean_name(&name);
    let name = if name.to_ascii_lowercase().ends_with(".html") {
        name
    } else {
        format!("{name}.html")
    };
    let copy = dir.join(name);
    std::fs::copy(abs, &copy).map_err(|e| format!("{}: {e}", copy.display()))?;
    Ok(copy)
}

/// Show a message to the user. A dialog on Windows, standard error elsewhere.
pub fn message_box(title: &str, text: &str, error: bool) {
    #[cfg(windows)]
    win::message_box(title, text, error);
    #[cfg(not(windows))]
    {
        let _ = error;
        eprintln!("{title}: {text}");
    }
}

#[cfg(windows)]
pub(crate) mod win {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::Command;
    use std::ptr::null_mut;

    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, SW_SHOWNORMAL,
    };
    use winreg::enums::{HKEY_CLASSES_ROOT, HKEY_CURRENT_USER};
    use winreg::RegKey;

    use super::{file_url, fill_args, is_chromium, split_command};

    pub fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
        s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
    }

    pub fn message_box(title: &str, text: &str, error: bool) {
        let style = MB_OK
            | if error {
                MB_ICONERROR
            } else {
                MB_ICONINFORMATION
            };
        let (text, title) = (wide(text), wide(title));
        unsafe {
            MessageBoxW(null_mut(), text.as_ptr(), title.as_ptr(), style);
        }
    }

    pub fn shell_open(path: &Path) -> Result<(), String> {
        let (verb, file) = (wide("open"), wide(path));
        let result = unsafe {
            ShellExecuteW(
                null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        // ShellExecuteW reports success with a value greater than 32.
        if result as isize > 32 {
            Ok(())
        } else {
            Err(format!("Windows could not open {}", path.display()))
        }
    }

    /// The default https handler's ProgId and its open command.
    fn default_browser() -> Option<(String, String)> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let progid: String = hkcu
            .open_subkey(
                r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https\UserChoice",
            )
            .ok()?
            .get_value("ProgId")
            .ok()?;
        let command: String = RegKey::predef(HKEY_CLASSES_ROOT)
            .open_subkey(format!(r"{progid}\shell\open\command"))
            .ok()?
            .get_value("")
            .ok()?;
        Some((progid, command))
    }

    /// Launch a Chromium-family default browser directly on the file. Returns false to fall back.
    pub fn open_with_default_browser(abs: &Path) -> bool {
        let Some((progid, command)) = default_browser() else {
            return false;
        };
        if !is_chromium(&progid) {
            return false;
        }
        let Some((program, template)) = split_command(&command) else {
            return false;
        };
        Command::new(program)
            .raw_arg(fill_args(&template, &file_url(abs)))
            .spawn()
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_file_urls() {
        assert_eq!(
            file_url(Path::new(r"D:\My Files\photo #1.jpg.bin2")),
            "file:///D:/My%20Files/photo%20%231.jpg.bin2"
        );
        assert_eq!(
            file_url(Path::new(r"\\server\share\a.bin2")),
            "file://server/share/a.bin2"
        );
        assert_eq!(
            file_url(Path::new("/home/me/ä.bin2")),
            "file:///home/me/%C3%A4.bin2"
        );
    }

    #[test]
    fn splits_registry_commands() {
        assert_eq!(
            split_command(r#""C:\Program Files\Edge\msedge.exe" --single-argument %1"#),
            Some((
                r"C:\Program Files\Edge\msedge.exe".to_string(),
                "--single-argument %1".to_string()
            ))
        );
        assert_eq!(
            split_command(r"C:\app.exe %1"),
            Some((r"C:\app.exe".to_string(), "%1".to_string()))
        );
        assert_eq!(split_command("  "), None);
    }

    #[test]
    fn fills_arguments() {
        assert_eq!(
            fill_args("--single-argument %1", "file:///a"),
            "--single-argument file:///a"
        );
        assert_eq!(fill_args("", "file:///a"), "file:///a");
        assert_eq!(fill_args("-osint -url \"%1\"", "x"), "-osint -url \"x\"");
    }

    #[test]
    fn knows_chromium_browsers() {
        assert!(is_chromium("MSEdgeHTM"));
        assert!(is_chromium("ChromeHTML"));
        assert!(!is_chromium("FirefoxURL-308046B0AF4A39CB"));
    }
}
