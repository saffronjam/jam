use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

const MAX_CHUNK: usize = 512 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PreviewRead {
    Inspect,
    Chunk {
        version: String,
        offset: u64,
        length: usize,
    },
    Line {
        version: String,
        number: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreviewFile {
    pub path: PathBuf,
    pub size: u64,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
}

fn version(file: &File) -> Result<(u64, String)> {
    let m = file.metadata()?;
    ensure!(m.is_file(), "Only regular files can be previewed");
    Ok((
        m.len(),
        format!(
            "{}:{}:{}:{}:{}",
            m.dev(),
            m.ino(),
            m.len(),
            m.mtime(),
            m.mtime_nsec()
        ),
    ))
}

pub(crate) fn read(cwd: &Path, path: &str, op: PreviewRead) -> Result<PreviewFile> {
    ensure!(!path.is_empty(), "Missing file path");
    let path = if let Some(rest) = path.strip_prefix("~/") {
        PathBuf::from(std::env::var_os("HOME").context("Home directory unavailable")?).join(rest)
    } else {
        cwd.join(path)
    };
    let path = path
        .canonicalize()
        .with_context(|| format!("Cannot find {}", path.display()))?;
    ensure!(
        path.metadata()?.is_file(),
        "Only regular files can be previewed"
    );
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(&path)
        .with_context(|| format!("Cannot read {}", path.display()))?;
    let (size, revision) = version(&file)?;
    let mut result = PreviewFile {
        path,
        size,
        version: revision,
        data: None,
        offset: None,
    };
    let expected = match &op {
        PreviewRead::Inspect => return Ok(result),
        PreviewRead::Line { version, .. } | PreviewRead::Chunk { version, .. } => version,
    };
    ensure!(
        expected == &result.version,
        "The file changed. Reload the preview."
    );
    match op {
        PreviewRead::Chunk { offset, length, .. } => {
            ensure!(
                (1..=MAX_CHUNK).contains(&length) && offset <= size,
                "Invalid file range"
            );
            file.seek(SeekFrom::Start(offset))?;
            let mut bytes = vec![0; (size - offset).min(length as u64) as usize];
            file.read_exact(&mut bytes)?;
            result.data = Some(STANDARD.encode(bytes));
        }
        PreviewRead::Line { number, .. } => {
            ensure!(number > 0, "Line numbers start at one");
            let mut reader = BufReader::new((&file).take(size));
            let mut line = 1;
            let mut offset = 0;
            while line < number {
                let bytes = reader.fill_buf()?;
                if bytes.is_empty() {
                    break;
                }
                let mut used = bytes.len();
                for (index, &byte) in bytes.iter().enumerate() {
                    if byte == b'\n' {
                        line += 1;
                        if line == number {
                            used = index + 1;
                            break;
                        }
                    }
                }
                reader.consume(used);
                offset += used as u64;
            }
            result.offset = Some(offset);
        }
        PreviewRead::Inspect => unreachable!(),
    }
    if version(&file)?.1 != result.version {
        bail!("The file changed. Reload the preview.")
    }
    Ok(result)
}

pub(crate) fn editor_command(path: &Path) -> Result<Vec<std::ffi::OsString>> {
    let setting = ["VISUAL", "EDITOR"]
        .into_iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "nano".into());
    let editor = shell_words::split(&setting).context("Invalid terminal editor setting")?;
    ensure!(!editor.is_empty(), "Empty terminal editor setting");
    // Only the editor gets the path. The PTY then becomes an interactive shell.
    let mut args: Vec<std::ffi::OsString> = [
        "/bin/sh",
        "-c",
        r#"trap : INT QUIT; "$@"; exec "${SHELL:-/bin/sh}" -i"#,
        "jam-editor",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    args.extend(editor.into_iter().map(Into::into));
    args.push(path.as_os_str().to_owned());
    Ok(args)
}
