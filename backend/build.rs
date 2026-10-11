use std::fs;

fn main() {
    // A desktop-only checkout does not require the server frontend.
    // Validate whichever frontends are present against the shared backend version.
    let cargo_version = env!("CARGO_PKG_VERSION");
    for manifest in ["../frontend/package.json", "../desktop/package.json"] {
        println!("cargo:rerun-if-changed={manifest}");
        if !std::path::Path::new(manifest).exists() { continue; }
        let package = fs::read_to_string(manifest).expect("Cannot read UI package manifest");
        let version = package.lines()
            .find(|line| line.trim().starts_with("\"version\""))
            .and_then(|line| line.split('"').nth(3))
            .expect("UI package manifest has no version");
        assert_eq!(version, cargo_version, "Version mismatch in {manifest}");
    }
    // The shared library also compiles the optional server module's embedded assets.
    fs::create_dir_all("../build_tmp/frontend/dist").expect("Cannot create server asset directory");
}
