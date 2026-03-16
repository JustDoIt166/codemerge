#[cfg(windows)]
fn main() {
    use std::path::PathBuf;

    println!("cargo:rerun-if-changed=assets/app.ico");

    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must exist"));
    let ico_path = manifest_dir.join("assets/app.ico");
    if !ico_path.exists() {
        panic!("missing Windows icon asset: {}", ico_path.display());
    }

    let mut res = winres::WindowsResource::new();
    res.set_icon(
        ico_path
            .to_str()
            .expect("ico path must be valid UTF-8 on Windows"),
    );

    if let Err(err) = res.compile() {
        panic!("failed to compile Windows resources: {err}");
    }
}

#[cfg(not(windows))]
fn main() {}
