#[cfg(windows)]
fn main() {
    use std::path::PathBuf;
    use std::process::Command;

    println!("cargo:rerun-if-changed=assets/fonts/logo.svg");

    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must exist"));
    let svg_path = manifest_dir.join("assets/fonts/logo.svg");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR must exist"));
    let ico_path = out_dir.join("codemerge.ico");

    let icon_ready = if ico_path.exists() {
        true
    } else {
        let status = Command::new("magick")
            .arg(svg_path.as_os_str())
            .args([
                "-background",
                "none",
                "-define",
                "icon:auto-resize=16,24,32,48,64,128,256",
            ])
            .arg(ico_path.as_os_str())
            .status();
        match status {
            Ok(exit) => exit.success(),
            Err(_) => false,
        }
    };

    if !icon_ready {
        println!(
            "cargo:warning=Windows icon generation skipped: install ImageMagick (magick) to embed logo.svg into codemerge.exe"
        );
        return;
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
