use std::{collections::hash_map::DefaultHasher, hash::Hasher, path::Path};
fn visit(path: &Path, root: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            visit(&path, root, files);
        } else {
            files.push(path.strip_prefix(root).unwrap().to_owned());
        }
    }
}
fn main() {
    let root = Path::new("../..");
    let mut files = vec!["Cargo.lock".into(), "Cargo.toml".into()];
    for directory in ["crates/jam-core/src", "crates/jam/src"] {
        visit(&root.join(directory), root, &mut files);
    }
    files.sort();
    let mut hash = DefaultHasher::new();
    for file in files {
        let path = root.join(&file);
        println!("cargo:rerun-if-changed={}", path.display());
        hash.write(file.to_string_lossy().as_bytes());
        hash.write(&std::fs::read(path).unwrap());
    }
    println!("cargo:rustc-env=JAM_BUILD_ID={:016x}", hash.finish());
}
