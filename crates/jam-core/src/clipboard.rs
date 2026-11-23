use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

pub(crate) fn store(png: &str) -> Result<PathBuf> {
    let directory =
        std::env::temp_dir().join(format!("jam-clipboard-{}", unsafe { libc::geteuid() }));
    store_in(&directory, png)
}

fn store_in(directory: &Path, png: &str) -> Result<PathBuf> {
    if png.len() > MAX_IMAGE_BYTES.div_ceil(3) * 4 {
        bail!("Clipboard image is too large. Maximum size is 16 MB");
    }
    let bytes = STANDARD
        .decode(png)
        .context("Invalid clipboard image encoding")?;
    if bytes.len() > MAX_IMAGE_BYTES {
        bail!("Clipboard image is too large. Maximum size is 16 MB");
    }
    if !bytes.starts_with(PNG_SIGNATURE) {
        bail!("Clipboard image must be a PNG");
    }
    crate::server::private_dir(directory)?;
    let now = SystemTime::now();
    // Agents may read an image after the paste has completed. Keep recent
    // files across daemon restarts, and prune only our old regular files.
    for entry in fs::read_dir(directory)?.flatten() {
        let name = entry.file_name();
        let Some(id) = name.to_str().and_then(|s| s.strip_suffix(".png")) else {
            continue;
        };
        if uuid::Uuid::parse_str(id).is_err() {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if metadata.is_file()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata
                .modified()
                .ok()
                .and_then(|at| now.duration_since(at).ok())
                .is_some_and(|age| age > RETENTION)
        {
            let _ = fs::remove_file(entry.path());
        }
    }
    let path = directory.join(format!("{}.png", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .context("Could not save clipboard image")?;
    if let Err(error) = file.write_all(&bytes) {
        let _ = fs::remove_file(&path);
        return Err(error).context("Could not save clipboard image");
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn images_are_private_unique_and_reject_invalid_content() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("clipboard");
        let png = STANDARD.encode([PNG_SIGNATURE, b"fixture"].concat());
        let first = store_in(&dir, &png).unwrap();
        let second = store_in(&dir, &png).unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read(&first).unwrap(), STANDARD.decode(&png).unwrap());
        assert_eq!(fs::metadata(&dir).unwrap().mode() & 0o777, 0o700);
        assert_eq!(fs::metadata(&first).unwrap().mode() & 0o777, 0o600);
        assert!(store_in(&dir, "not base64").is_err());
        assert!(store_in(&dir, &STANDARD.encode(b"not a PNG")).is_err());
        assert!(store_in(&dir, &"a".repeat(MAX_IMAGE_BYTES.div_ceil(3) * 4 + 1)).is_err());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
    }
    #[test]
    fn refuses_a_symlink_directory() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        fs::create_dir(&target).unwrap();
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(store_in(&link, &STANDARD.encode(PNG_SIGNATURE)).is_err());
        assert_eq!(fs::read_dir(target).unwrap().count(), 0);
    }

    #[test]
    fn retention_only_removes_old_owned_images() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("clipboard");
        let png = STANDARD.encode(PNG_SIGNATURE);
        let old = store_in(&directory, &png).unwrap();
        let recent = store_in(&directory, &png).unwrap();
        let foreign = directory.join("keep.png");
        fs::write(&foreign, PNG_SIGNATURE).unwrap();
        let outside = root.path().join("outside");
        fs::write(&outside, PNG_SIGNATURE).unwrap();
        let link = directory.join(format!("{}.png", uuid::Uuid::new_v4()));
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let time = SystemTime::now() - RETENTION - Duration::from_secs(1);
        for path in [&old, &foreign] {
            fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(time))
                .unwrap();
        }
        store_in(&directory, &png).unwrap();
        assert!(!old.exists());
        assert!(recent.exists());
        assert!(foreign.exists());
        assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
        assert!(outside.exists());
    }
}
