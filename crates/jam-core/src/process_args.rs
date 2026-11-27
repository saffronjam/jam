//! Read process arguments with OS-provided argument boundaries.
use std::io;

pub(crate) fn arguments(pid: u32) -> io::Result<Vec<String>> {
    #[cfg(target_os = "linux")]
    let bytes = std::fs::read(format!("/proc/{pid}/cmdline"))?;
    #[cfg(target_os = "macos")]
    let (storage, range) = arguments_and_environment(pid)?;
    #[cfg(target_os = "macos")]
    let bytes = &storage[range];
    let Some(args) = bytes.strip_suffix(&[0]) else {
        return Ok(Vec::new());
    };
    Ok(args
        .split(|b| *b == 0)
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect())
}

#[cfg(target_os = "macos")]
pub(crate) fn arguments_and_environment(
    pid: u32,
) -> std::io::Result<(Vec<u8>, std::ops::Range<usize>)> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as i32];
    let mut size: usize = 0;
    unsafe {
        if libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return Err(std::io::Error::last_os_error());
        }
    }
    if !(4..=2 * 1024 * 1024).contains(&size) {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let mut bytes = vec![0u8; size];
    unsafe {
        if libc::sysctl(
            mib.as_mut_ptr(),
            3,
            bytes.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return Err(std::io::Error::last_os_error());
        }
    }
    bytes.truncate(size);
    let argc = i32::from_ne_bytes(
        bytes
            .get(..4)
            .and_then(|s| s.try_into().ok())
            .ok_or(std::io::ErrorKind::InvalidData)?,
    );
    if !(0..=65536).contains(&argc) {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let mut cursor = 4;
    while cursor < bytes.len() && bytes[cursor] != 0 {
        cursor += 1;
    }
    while cursor < bytes.len() && bytes[cursor] == 0 {
        cursor += 1;
    }
    let start = cursor;
    for _ in 0..argc {
        let length = bytes[cursor..]
            .iter()
            .position(|b| *b == 0)
            .ok_or(std::io::ErrorKind::InvalidData)?;
        cursor += length + 1;
    }
    Ok((bytes, start..cursor))
}
