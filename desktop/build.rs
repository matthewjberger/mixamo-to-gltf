fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set");
    let dist = std::path::Path::new(&manifest_dir).join("../dist");
    std::fs::create_dir_all(&dist).expect("failed to create the dist directory");
}
