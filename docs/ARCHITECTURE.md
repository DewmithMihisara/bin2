# bin2 Architecture

bin2 wraps any file together with its own viewer, so the file opens anywhere, forever.

## Core idea

bin2 is installed like WinRAR: right-click menus, a desktop app, a `.bin2` file association.
Inside, every `.bin2` file is a **single self-contained HTML page**.
That is the safety net: if bin2 is not installed, or one day stops existing, any web browser still opens the file, offline.
The browser is the most durable runtime ever made. A 1995 HTML page still opens today on every device.

The first lines of every `.bin2` file are plain text, so even Notepad explains what it is and how to recover the original by hand.

## What is inside one bin2 file

1. **Original bytes.** Base64-encoded, gzip-compressed when that helps. Lossless, with a "Download original" button.
2. **Live viewer.** JavaScript (later plus WebAssembly decoders), embedded inline. WASM loads from bytes, so it works over `file://`.
3. **Static fallback.** Something readable even with scripts disabled: the image itself, the text itself, or recovery instructions.
4. **Manifest.** JSON: original name, size, type, SHA-256, encoding, sealing date, tool version.

```text
<!-- plain-text notice + manual recovery instructions -->
<!doctype html>
... toolbar, static fallback ...
<script type="application/json" id="bin2-manifest">{ ... }</script>
<script type="application/octet-stream" id="bin2-payload">BASE64...</script>   (or <img id="bin2-payload" src="data:..."> for images)
<script> viewer </script>
```

For browser-native images, the payload is an `<img>` data URI. The picture shows even with JavaScript off, and the bytes are stored only once.

## Decoders for formats browsers cannot show

| File type | Engine |
|---|---|
| HEIC | libheif |
| Camera RAW (CR2, NEF, ARW) | LibRaw |
| PSD | ag-psd |
| DOCX | mammoth.js |
| Old docs (WordPerfect, Works, Pages, CorelDraw) | Document Liberation Project libs (libwpd, libwps, libetonyek, libcdr) |
| Old games and programs | js-dos / v86 emulation |

The old-format decoders are the real differentiator: they make "a file from 2003 opens in 2050" true.
PDF, common images, audio, video, text and Markdown already work through the browser itself.

## Installed experience (Windows)

### Built (v0.1)

`bin2 install` sets everything up for the current user only, under `HKEY_CURRENT_USER`, with no admin rights:

- **Files** go to `%LOCALAPPDATA%\Programs\bin2`: `bin2.exe` (command line) and `bin2w.exe` (windowless helper for Explorer).
- **`.bin2` type** is registered with `Content Type = text/html`, so browsers render it as a page instead of raw text.
- **Double-click** a `.bin2` runs `bin2w open`, which launches a Chromium-family default browser on the file. Other browsers get a temporary `.html` copy.
- **Right-click a `.bin2`:** Open with bin2, Extract with bin2.
- **Right-click any file:** Seal with bin2, Seal with bin2 (shareable .html). Name clashes become `name (2).bin2`, like Explorer.
- **PATH** gains the install folder, keeping the value's registry type and any `%VARIABLES%`.
- **Apps & features** lists bin2 with an uninstall entry.

`bin2 uninstall` removes exactly what install added, and can remove itself while running.
`bin2w` never shows a console window. It is silent on success and shows a message box on errors.

On Windows 11, classic shell verbs sit under "Show more options". The top-level menu needs a packaged app.

### Planned

- **Desktop app:** Tauri (Rust + Windows WebView2), reusing the same viewers.
- **Windows 11 top-level menu:** an `IExplorerCommand` handler in an MSIX or sparse package.
- **More shell integration:** "Seal folder" on folders, Explorer thumbnails from the static fallback.
- **App features:** view, extract, open original in its native app, batch seal, re-seal old files with newer viewers.
- **Installer:** MSI or NSIS via the Tauri bundler, replacing `bin2 install`.

## Code layout

| Path | Purpose |
|---|---|
| `crates/bin2-core/` | Type detection, sealing, unsealing, manifest |
| `crates/bin2-cli/` | The `bin2` command-line tool, the `bin2w` Explorer helper, install and uninstall |
| `assets/` | The bin2 icon and the script that draws it |
| `template/shell.html` | Page shell every bin2 file is built from |
| `template/viewer.js` | Viewer inlined into every bin2 file |
| `viewers/<type>/` | Future WASM decoder plugins, one per format |

## Realistic risks

- **Size overhead.** Base64 adds about a third, and future WASM viewers add 1–25 MB. A later "thin" mode can share decoders by hash.
- **Large files.** Browsers struggle above about 1–2 GB in memory. bin2 targets documents, photos and saves, not movies.
- **Licensing.** Decoders get embedded in output files. Prefer MIT/BSD/MPL engines and ship license text in the manifest.
- **Browser policy.** Browsers could restrict local HTML someday. The plain-text recovery notice protects against that.
- **Unknown extension.** Some browsers may download a `.bin2` instead of rendering it. The installer's content-type registration and renaming to `.html` both work around it.
- **Prior art.** SingleFile, Webrecorder and Internet Archive emulation exist, but none packs arbitrary files with their own decoder.

## Roadmap

- **v0.1 (now):** core format, `bin2 seal / extract / info` CLI, browser-native viewers (images, PDF, audio, video, text, Markdown, HTML), hex view for everything else. Per-user Windows install with right-click menus, double-click open, icon and PATH.
- **v0.2:** WASM decoders for HEIC, RAW, PSD, TIFF, DOCX. Folder sealing. Tauri app and a real installer.
- **v0.3:** old document formats via Document Liberation Project libs.
- **v0.4:** emulation for old programs and game saves. "Thin" mode with shared decoders.
