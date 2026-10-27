//! Sample restoration never follows symlinks or truncates a file before checking its type.
use std::{
    fs::File,
    io::{self, Read, Seek, Write},
    path::Path,
};

#[cfg(unix)]
fn open(path: &Path) -> io::Result<File> {
    use rustix::fs::{openat, Mode, OFlags};
    use std::path::Component;
    if !path.is_absolute() {
        return Err(io::Error::other("invalid sample path"));
    }
    let mut file = File::open("/")?;
    let mut parts = path.components().peekable();
    while let Some(part) = parts.next() {
        let Component::Normal(name) = part else {
            if part == Component::RootDir {
                continue;
            }
            return Err(io::Error::other("invalid sample path"));
        };
        // Pin every ancestor, not only the final file. No TRUNC until fstat
        // confirms a regular file, so a link/FIFO/directory cannot be modified.
        let flags = OFlags::NOFOLLOW
            | OFlags::CLOEXEC
            | OFlags::NONBLOCK
            | if parts.peek().is_some() {
                OFlags::RDONLY | OFlags::DIRECTORY
            } else {
                OFlags::RDWR | OFlags::CREATE
            };
        file = File::from(openat(&file, name, flags, Mode::RUSR | Mode::WUSR)?);
    }
    Ok(file)
}

#[cfg(not(unix))]
fn open(path: &Path) -> io::Result<File> {
    use std::fs::OpenOptions;
    for ancestor in path.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(io::Error::other("sample links are refused"))
            }
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    options.open(path)
}

pub(crate) fn restore(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("sample must be a regular file"));
    }
    let mut current = Vec::new();
    // Bound the read even if another process grows a previously edited sample.
    Read::by_ref(&mut file)
        .take(bytes.len() as u64 + 1)
        .read_to_end(&mut current)?;
    if current != bytes {
        file.rewind()?;
        file.set_len(0)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    Ok(())
}
