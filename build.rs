#[cfg(target_os = "windows")]
fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let icon_path = std::path::PathBuf::from(&manifest_dir)
        .join("assets")
        .join("app.ico");
    println!("cargo:rerun-if-changed={}", icon_path.display());

    let mut res = winresource::WindowsResource::new();

    let dpi_manifest = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2,PerMonitor</dpiAwareness>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/PM</dpiAware>
    </windowsSettings>
  </application>
</assembly>"#;
    res.set_manifest(dpi_manifest);

    if icon_path.exists() {
        let icon_path = std::fs::canonicalize(&icon_path).unwrap_or(icon_path);
        let icon_path_str = icon_path
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .to_string();
        res.set_icon(&icon_path_str);
    } else {
        println!(
            "cargo:warning=icon file not found at {}; building without embedded Windows icon",
            icon_path.display()
        );
    }

    res.compile()
        .expect("failed to embed Windows resources (manifest/icon)");
}

#[cfg(not(target_os = "windows"))]
fn main() {}
