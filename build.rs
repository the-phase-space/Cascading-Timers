#[cfg(target_os = "windows")]
fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let icon_path = std::path::PathBuf::from(&manifest_dir)
        .join("assets")
        .join("app.ico");
    println!("cargo:rerun-if-changed={}", icon_path.display());

    if !icon_path.exists() {
        println!(
            "cargo:warning=icon file not found at {}; building without embedded Windows icon",
            icon_path.display()
        );
        return;
    }

    let icon_path = std::fs::canonicalize(&icon_path).unwrap_or(icon_path);
    let icon_path_str = icon_path
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string();

    let mut res = winresource::WindowsResource::new();
    res.set_icon(&icon_path_str);
    res.compile()
        .expect("failed to embed Windows icon resource");
}

#[cfg(not(target_os = "windows"))]
fn main() {}
