#[cfg(target_os = "windows")]
fn main() {
    let icon_path = "../Untitled-5(1).ico";
    println!("cargo:rerun-if-changed={icon_path}");

    let mut res = winresource::WindowsResource::new();
    res.set_icon(icon_path);
    res.compile()
        .expect("failed to embed Windows icon resource from ../Untitled-5(1).ico");
}

#[cfg(not(target_os = "windows"))]
fn main() {}
