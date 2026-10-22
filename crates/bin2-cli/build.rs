//! Embed the bin2 icon and version details into the Windows executables.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/bin2.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/bin2.ico")
        .set("ProductName", "bin2")
        .set("FileDescription", "bin2: files that never die")
        .set(
            "LegalCopyright",
            "© 2026 Dewmith Mihisara. All rights reserved.",
        );
    if let Err(e) = res.compile() {
        println!("cargo:warning=could not embed the bin2 icon: {e}");
    }
}
