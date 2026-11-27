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
    for name in [
        "JAM_FIXTURE_ORIGIN",
        "JAM_FIXTURE_CA_FILE",
        "JAM_FIXTURE_UPDATE_KEY_FILE",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
        if std::env::var_os("CARGO_FEATURE_FIXTURE_SERVICES").is_some() {
            let value = std::env::var(name)
                .unwrap_or_else(|_| panic!("{name} is required for fixture-services"));
            hash.write(name.as_bytes());
            if name.ends_with("_FILE") {
                let path = Path::new(&value);
                assert!(path.is_absolute(), "{name} must be absolute");
                println!("cargo:rerun-if-changed={value}");
                hash.write(&std::fs::read(path).expect("fixture trust file must be readable"));
            } else {
                hash.write(value.as_bytes());
                let port = value
                    .strip_prefix("https://localhost:")
                    .or_else(|| value.strip_prefix("https://127.0.0.1:"))
                    .and_then(|port| port.parse::<u16>().ok());
                assert!(
                    port.is_some_and(|port| port > 0),
                    "fixture origin must be local HTTPS with a numeric port and no path"
                );
            }
        }
    }
    for file in files {
        let path = root.join(&file);
        println!("cargo:rerun-if-changed={}", path.display());
        hash.write(file.to_string_lossy().as_bytes());
        hash.write(&std::fs::read(path).unwrap());
    }
    println!("cargo:rustc-env=JAM_BUILD_ID={:016x}", hash.finish());
}
