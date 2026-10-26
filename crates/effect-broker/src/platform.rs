//! Filesystem boundary. Unsafe blocks are restricted to documented OS FFI.
use crate::{BrokerError, FileIdentity};
use std::fs::File;
#[cfg(windows)]
use std::fs::OpenOptions;
use std::io;
use std::path::Path;

#[cfg(unix)]
pub fn open(path: &Path, directory: bool) -> Result<File, BrokerError> {
    use rustix::fs::{openat, Mode, OFlags};
    use std::path::Component;
    if !path.is_absolute() {
        return Err(BrokerError::InvalidSelection);
    }
    let mut file = File::open("/")?;
    let mut components = path.components().peekable();
    while let Some(component) = components.next() {
        let name = match component {
            Component::RootDir => continue,
            Component::Normal(name) => name,
            _ => return Err(BrokerError::InvalidSelection),
        };
        let is_directory = directory || components.peek().is_some();
        // Walk each component relative to a pinned FD. NOFOLLOW on just the
        // final component would leave an ancestor-symlink race.
        let flags = OFlags::RDONLY
            | OFlags::NOFOLLOW
            | OFlags::NONBLOCK
            | OFlags::CLOEXEC
            | if is_directory {
                OFlags::DIRECTORY
            } else {
                OFlags::empty()
            };
        file = File::from(openat(&file, name, flags, Mode::empty()).map_err(io::Error::from)?);
    }
    let metadata = file.metadata()?;
    if (directory && !metadata.is_dir()) || (!directory && !metadata.is_file()) {
        return Err(BrokerError::InvalidSelection);
    }
    Ok(file)
}

#[cfg(unix)]
pub fn identity(file: &File) -> Result<FileIdentity, BrokerError> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok(FileIdentity {
        dev: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(unix)]
pub fn mtime(file: &File) -> Result<(i64, i64), BrokerError> {
    use std::os::unix::fs::MetadataExt;
    let m = file.metadata()?;
    Ok((m.mtime(), m.mtime_nsec()))
}

#[cfg(unix)]
pub fn create_temp(root: &File, name: &str) -> Result<File, BrokerError> {
    use rustix::fs::{openat, Mode, OFlags};
    let fd = openat(
        root,
        name,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )
    .map_err(io::Error::from)?;
    Ok(File::from(fd))
}

#[cfg(unix)]
pub fn open_child(root: &File, name: &str) -> Result<File, BrokerError> {
    use rustix::fs::{openat, Mode, OFlags};
    let fd = openat(
        root,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(io::Error::from)?;
    let file = File::from(fd);
    if !file.metadata()?.is_file() {
        return Err(BrokerError::InvalidSelection);
    }
    Ok(file)
}

#[cfg(unix)]
pub fn remove_temp(root: &File, name: &str) {
    let _ = rustix::fs::unlinkat(root, name, rustix::fs::AtFlags::empty());
}

#[cfg(unix)]
pub fn destination_exists(root: &File, name: &str) -> Result<bool, BrokerError> {
    match rustix::fs::statat(root, name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW) {
        Ok(_) => Ok(true),
        Err(rustix::io::Errno::NOENT) => Ok(false),
        Err(e) => Err(io::Error::from(e).into()),
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn publish(root: &File, temporary: &str, destination: &str) -> Result<(), BrokerError> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    let temporary = CString::new(temporary).map_err(|_| BrokerError::InvalidBasename)?;
    let destination = CString::new(destination).map_err(|_| BrokerError::InvalidBasename)?;
    // SAFETY: both C strings live through the call; root owns a live directory FD.
    // The *at variant pins both paths to that FD despite concurrent parent renames.
    #[cfg(target_os = "macos")]
    let result = unsafe {
        libc::renameatx_np(
            root.as_raw_fd(),
            temporary.as_ptr(),
            root.as_raw_fd(),
            destination.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    // SAFETY: same live strings/FD invariants. RENAME_NOREPLACE is atomic and
    // must be supported by the filesystem; there is no check-then-rename fallback.
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            root.as_raw_fd(),
            temporary.as_ptr(),
            root.as_raw_fd(),
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::EEXIST) | Some(libc::ENOTEMPTY) => Err(BrokerError::DestinationExists),
        Some(libc::ENOSYS) | Some(libc::EINVAL) | Some(libc::EOPNOTSUPP) => {
            Err(BrokerError::UnsupportedAtomicNoReplace)
        }
        _ => Err(error.into()),
    }
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
pub fn publish(_: &File, _: &str, _: &str) -> Result<(), BrokerError> {
    Err(BrokerError::UnsupportedAtomicNoReplace)
}

#[cfg(unix)]
pub fn sync_directory(root: &File) -> Result<(), BrokerError> {
    root.sync_all()?;
    Ok(())
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    };
    use windows_sys::Win32::Storage::FileSystem::*;

    pub fn open(path: &Path, directory: bool) -> Result<File, BrokerError> {
        // No FILE_SHARE_DELETE: hold the selected file/directory against rename
        // while in use. OPEN_REPARSE_POINT prevents following the last component.
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(
                FILE_FLAG_OPEN_REPARSE_POINT
                    | if directory {
                        FILE_FLAG_BACKUP_SEMANTICS
                    } else {
                        0
                    },
            )
            .open(path)?;
        let m = file.metadata()?;
        if m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || (directory && !m.is_dir())
            || (!directory && !m.is_file())
        {
            return Err(BrokerError::InvalidSelection);
        }
        Ok(file)
    }
    pub fn identity(file: &File) -> Result<FileIdentity, BrokerError> {
        let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
        // SAFETY: file owns a live HANDLE and info points to writable storage of
        // the exact ABI type. Only read it after the API reports success.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        // SAFETY: successful GetFileInformationByHandle initialized all fields.
        let info = unsafe { info.assume_init() };
        Ok(FileIdentity {
            dev: u64::from(info.dwVolumeSerialNumber),
            inode: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        })
    }
    pub fn mtime(file: &File) -> Result<(i64, i64), BrokerError> {
        let ticks = file.metadata()?.last_write_time();
        Ok((
            (ticks / 10_000_000) as i64,
            ((ticks % 10_000_000) * 100) as i64,
        ))
    }
    // Windows relative opens are represented by the resolved final directory
    // path. Its open handle excludes delete-sharing, preventing root swaps.
    fn child(root: &File, name: &str) -> Result<std::path::PathBuf, BrokerError> {
        let mut buffer = vec![0u16; 32768];
        // SAFETY: live directory HANDLE, writable buffer, length in UTF-16 units.
        let len = unsafe {
            GetFinalPathNameByHandleW(
                root.as_raw_handle(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                0,
            )
        } as usize;
        if len == 0 || len >= buffer.len() {
            return Err(io::Error::last_os_error().into());
        }
        use std::os::windows::ffi::OsStringExt;
        Ok(std::path::PathBuf::from(std::ffi::OsString::from_wide(&buffer[..len])).join(name))
    }
    pub fn create_temp(root: &File, name: &str) -> Result<File, BrokerError> {
        Ok(OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(child(root, name)?)?)
    }
    pub fn open_child(root: &File, name: &str) -> Result<File, BrokerError> {
        open(&child(root, name)?, false)
    }
    pub fn remove_temp(root: &File, name: &str) {
        if let Ok(path) = child(root, name) {
            let _ = std::fs::remove_file(path);
        }
    }
    pub fn destination_exists(root: &File, name: &str) -> Result<bool, BrokerError> {
        match std::fs::symlink_metadata(child(root, name)?) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
    pub fn publish(root: &File, temporary: &str, destination: &str) -> Result<(), BrokerError> {
        use std::os::windows::ffi::OsStrExt;
        let src: Vec<_> = child(root, temporary)?
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let dst: Vec<_> = child(root, destination)?
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        // SAFETY: NUL-terminated UTF-16 buffers live for the call. No REPLACE or
        // COPY_ALLOWED flag: same-directory atomic move, existing targets fail.
        if unsafe { MoveFileExW(src.as_ptr(), dst.as_ptr(), MOVEFILE_WRITE_THROUGH) } != 0 {
            return Ok(());
        }
        let e = io::Error::last_os_error();
        match e.raw_os_error().map(|v| v as u32) {
            Some(windows_sys::Win32::Foundation::ERROR_ALREADY_EXISTS)
            | Some(windows_sys::Win32::Foundation::ERROR_FILE_EXISTS) => {
                Err(BrokerError::DestinationExists)
            }
            Some(windows_sys::Win32::Foundation::ERROR_NOT_SUPPORTED)
            | Some(windows_sys::Win32::Foundation::ERROR_INVALID_FUNCTION) => {
                Err(BrokerError::UnsupportedAtomicNoReplace)
            }
            _ => Err(e.into()),
        }
    }
    pub fn sync_directory(root: &File) -> Result<(), BrokerError> {
        root.sync_all()?;
        Ok(())
    }
}
#[cfg(windows)]
pub use windows::*;
