//! Private development vault. Unix permissions are required; other platforms fail closed.
use super::{Secret, SecretRef, SecretStore, SecretStoreKind, VaultError};
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;

pub struct FileStore {
    #[cfg(unix)]
    directory: PathBuf,
}

impl std::fmt::Debug for FileStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileStore(<private>)")
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};
    use std::{
        collections::BTreeMap,
        ffi::{CStr, CString},
        fs::{self, DirBuilder, File, OpenOptions},
        io::{self, Read, Write},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
        },
        sync::atomic::{AtomicU64, Ordering},
    };
    use zeroize::Zeroizing;

    type Entries = BTreeMap<String, Zeroizing<String>>;
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    const SECRETS: &CStr = c"secrets.json";

    fn unavailable(_: impl std::fmt::Debug) -> VaultError {
        VaultError::Unavailable
    }

    fn check_private(file: &File, directory: bool) -> io::Result<()> {
        let m = file.metadata()?;
        // SAFETY: geteuid takes no arguments and has no preconditions.
        check_metadata(&m, directory, unsafe { libc::geteuid() })
    }

    fn check_metadata(m: &fs::Metadata, directory: bool, owner: u32) -> io::Result<()> {
        let allowed = if directory { 0o700 } else { 0o600 };
        if m.uid() != owner
            || m.mode() & 0o7777 & !allowed != 0
            || (directory && !m.is_dir())
            || (!directory && (!m.is_file() || m.nlink() != 1))
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(())
    }

    /// Resolve all children relative to a validated directory descriptor. O_NOFOLLOW
    /// and fstat validate the object actually opened, avoiding check/open races.
    fn open_at(dir: &File, name: &CStr, flags: i32) -> io::Result<File> {
        // SAFETY: valid directory fd and NUL-terminated name; mode is supplied for O_CREAT.
        let fd = unsafe {
            libc::openat(
                dir.as_raw_fd(),
                name.as_ptr(),
                flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                0o600 as libc::c_uint,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: openat returned a new owned fd.
        let file = unsafe { File::from_raw_fd(fd) };
        check_private(&file, false)?;
        Ok(file)
    }

    fn read(dir: &File) -> io::Result<Entries> {
        let mut file = match open_at(dir, SECRETS, libc::O_RDONLY) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Entries::new()),
            Err(e) => return Err(e),
        };
        let mut bytes = Zeroizing::new(Vec::new());
        file.read_to_end(&mut bytes)?;
        let entries: Entries = serde_json::from_slice(&bytes)?;
        for (name, value) in &entries {
            SecretRef::new(name.clone()).map_err(|_| io::ErrorKind::InvalidData)?;
            let _decoded = Zeroizing::new(
                STANDARD
                    .decode(value.as_bytes())
                    .map_err(|_| io::ErrorKind::InvalidData)?,
            );
        }
        Ok(entries)
    }

    fn write(dir: &File, entries: &Entries) -> io::Result<()> {
        let bytes = Zeroizing::new(serde_json::to_vec(entries)?);
        for _ in 0..128 {
            let name = CString::new(format!(
                ".secrets-{}-{}.tmp",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ))
            .expect("generated name has no NUL");
            let mut temp = match open_at(dir, &name, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL)
            {
                Ok(file) => file,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            };
            let result = (|| {
                temp.write_all(&bytes)?;
                temp.sync_all()?;
                // Recheck the destination before replacement (never repair unsafe permissions).
                match open_at(dir, SECRETS, libc::O_RDONLY) {
                    Ok(_) => {}
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e),
                }
                // SAFETY: both names and the directory fd remain valid during renameat.
                if unsafe {
                    libc::renameat(
                        dir.as_raw_fd(),
                        name.as_ptr(),
                        dir.as_raw_fd(),
                        SECRETS.as_ptr(),
                    )
                } != 0
                {
                    return Err(io::Error::last_os_error());
                }
                dir.sync_all()
            })();
            // SAFETY: unlink only our exclusive temp name. After rename it no longer exists.
            unsafe { libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), 0) };
            return result;
        }
        Err(io::ErrorKind::AlreadyExists.into())
    }

    impl FileStore {
        pub fn new(directory: impl AsRef<Path>) -> Result<Self, VaultError> {
            // Strip trailing separators so O_NOFOLLOW also covers a caller's "dir/".
            let directory: PathBuf = directory.as_ref().components().collect();
            // Never follow a pre-existing symlink, including a dangling one.
            match fs::symlink_metadata(&directory) {
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    DirBuilder::new()
                        .recursive(true)
                        .mode(0o700)
                        .create(&directory)
                        .map_err(unavailable)?;
                }
                Err(e) => return Err(unavailable(e)),
            }
            let store = Self { directory };
            store.with_lock(|dir| read(dir).map(|_| ()))?;
            Ok(store)
        }

        fn with_lock<T>(&self, op: impl FnOnce(&File) -> io::Result<T>) -> Result<T, VaultError> {
            let dir = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&self.directory)
                .map_err(unavailable)?;
            check_private(&dir, true).map_err(unavailable)?;
            // Keep the lock file permanently: unlinking it would split concurrent locks.
            let lock =
                open_at(&dir, c".lock", libc::O_RDWR | libc::O_CREAT).map_err(unavailable)?;
            loop {
                // SAFETY: lock owns a valid fd. Dropping it releases the advisory flock.
                if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } == 0 {
                    break;
                }
                let e = io::Error::last_os_error();
                if e.kind() != io::ErrorKind::Interrupted {
                    return Err(unavailable(e));
                }
            }
            op(&dir).map_err(unavailable)
        }
    }

    impl SecretStore for FileStore {
        fn kind(&self) -> SecretStoreKind {
            SecretStoreKind::File
        }
        fn get(&self, reference: &SecretRef) -> Result<Option<Secret>, VaultError> {
            self.with_lock(|dir| {
                read(dir)?
                    .get(reference.name())
                    .map(|value| {
                        STANDARD
                            .decode(value.as_bytes())
                            .map(Secret::new)
                            .map_err(|_| io::ErrorKind::InvalidData.into())
                    })
                    .transpose()
            })
        }
        fn set(&self, reference: &SecretRef, secret: Secret) -> Result<(), VaultError> {
            self.with_lock(|dir| {
                let mut entries = read(dir)?;
                entries.insert(
                    reference.name().to_owned(),
                    Zeroizing::new(STANDARD.encode(secret.expose())),
                );
                write(dir, &entries)
            })
        }
        fn delete(&self, reference: &SecretRef) -> Result<(), VaultError> {
            self.with_lock(|dir| {
                let mut entries = read(dir)?;
                if entries.remove(reference.name()).is_some() {
                    write(dir, &entries)?;
                }
                Ok(())
            })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{
            os::unix::fs::{symlink, PermissionsExt},
            sync::{Arc, Barrier},
        };

        const CANARY: &[u8] = b"private-canary-1234";

        fn fixture() -> (tempfile::TempDir, PathBuf, SecretRef) {
            let tmp = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
            let path = tmp.path().join("vault");
            (tmp, path, SecretRef::new("test-key").unwrap())
        }

        fn assert_unavailable<T>(result: Result<T, VaultError>) {
            let e = result.err().expect("unsafe vault must be rejected");
            assert_eq!(e, VaultError::Unavailable);
            assert_eq!(e.to_string(), "credential store unavailable");
            assert_eq!(format!("{e:?}"), "Unavailable");
        }

        #[test]
        fn round_trip_restart_replace_and_delete() {
            let (_tmp, path, r) = fixture();
            let store = FileStore::new(&path).unwrap();
            assert_eq!(store.kind(), SecretStoreKind::File);
            assert!(store.get(&r).unwrap().is_none());
            let binary = [0, 255, b'\n', b'"'];
            store.set(&r, Secret::new(binary.to_vec())).unwrap();
            assert_eq!(store.get(&r).unwrap().unwrap().expose(), binary);
            store.set(&r, Secret::new(CANARY.to_vec())).unwrap();
            let restarted = FileStore::new(&path).unwrap();
            assert_eq!(restarted.get(&r).unwrap().unwrap().expose(), CANARY);
            let json: serde_json::Value =
                serde_json::from_slice(&fs::read(path.join("secrets.json")).unwrap()).unwrap();
            assert_eq!(json[r.name()], STANDARD.encode(CANARY));
            for (p, mode) in [
                (&path, 0o700),
                (&path.join("secrets.json"), 0o600),
                (&path.join(".lock"), 0o600),
            ] {
                assert_eq!(fs::metadata(p).unwrap().mode() & 0o777, mode);
            }
            restarted.delete(&r).unwrap();
            restarted.delete(&r).unwrap();
            assert!(FileStore::new(&path).unwrap().get(&r).unwrap().is_none());
            assert_eq!(
                fs::read_dir(&path).unwrap().count(),
                2,
                "no temp files left behind"
            );
        }

        #[test]
        fn refuses_broad_permissions_on_directory_secrets_and_lock_without_repairing_them() {
            for target in ["", "secrets.json", ".lock"] {
                for mode in [0o644, 0o660, 0o755, 0o777, 0o1700]
                    .into_iter()
                    .chain((!target.is_empty()).then_some(0o700))
                {
                    let (_tmp, path, r) = fixture();
                    let store = FileStore::new(&path).unwrap();
                    store.set(&r, Secret::new(CANARY.to_vec())).unwrap();
                    let target_path = if target.is_empty() {
                        path.clone()
                    } else {
                        path.join(target)
                    };
                    fs::set_permissions(&target_path, fs::Permissions::from_mode(mode)).unwrap();
                    assert_unavailable(FileStore::new(&path));
                    assert_unavailable(store.get(&r));
                    assert_unavailable(store.set(&r, Secret::new(b"replacement".to_vec())));
                    assert_unavailable(store.delete(&r));
                    assert_eq!(fs::metadata(&target_path).unwrap().mode() & 0o7777, mode);
                    // Restore directory permissions so the temp fixture can be removed.
                    if target.is_empty() {
                        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
                    }
                }
            }
        }

        #[test]
        fn ownership_validation_rejects_another_user() {
            let (_tmp, path, r) = fixture();
            let store = FileStore::new(&path).unwrap();
            store.set(&r, Secret::new(CANARY.to_vec())).unwrap();
            for (p, is_dir) in [
                (&path, true),
                (&path.join("secrets.json"), false),
                (&path.join(".lock"), false),
            ] {
                let meta = fs::metadata(p).unwrap();
                assert!(check_metadata(&meta, is_dir, meta.uid()).is_ok());
                // Exercise the exact metadata check without requiring privileged chown.
                assert!(check_metadata(&meta, is_dir, meta.uid().wrapping_add(1)).is_err());
            }
        }

        #[test]
        fn refuses_symlink_directory_even_with_a_trailing_separator() {
            let (tmp, path, _r) = fixture();
            let real = tmp.path().join("real");
            FileStore::new(&real).unwrap();
            symlink(&real, &path).unwrap();
            assert_unavailable(FileStore::new(&path));
            assert_unavailable(FileStore::new(path.join("")));
            let dangling = tmp.path().join("dangling");
            symlink(tmp.path().join("missing"), &dangling).unwrap();
            assert_unavailable(FileStore::new(&dangling));
        }

        #[test]
        fn refuses_symlink_secrets_and_lock_without_touching_target() {
            for name in ["secrets.json", ".lock"] {
                let (tmp, path, r) = fixture();
                let store = FileStore::new(&path).unwrap();
                store.set(&r, Secret::new(CANARY.to_vec())).unwrap();
                let target = tmp.path().join("target");
                fs::write(&target, CANARY).unwrap();
                fs::remove_file(path.join(name)).unwrap();
                symlink(&target, path.join(name)).unwrap();
                assert_unavailable(FileStore::new(&path));
                assert_unavailable(store.get(&r));
                assert_unavailable(store.set(&r, Secret::new(b"replacement".to_vec())));
                assert_unavailable(store.delete(&r));
                assert_eq!(fs::read(&target).unwrap(), CANARY);
            }
        }

        #[test]
        fn rejects_non_regular_files_and_hardlinks() {
            let (tmp, path, r) = fixture();
            let store = FileStore::new(&path).unwrap();
            store.set(&r, Secret::new(CANARY.to_vec())).unwrap();
            fs::hard_link(path.join("secrets.json"), tmp.path().join("alias")).unwrap();
            assert_unavailable(store.get(&r));
            fs::remove_file(path.join("secrets.json")).unwrap();
            fs::create_dir(path.join("secrets.json")).unwrap();
            assert_unavailable(store.get(&r));
            assert_unavailable(store.set(&r, Secret::new(CANARY.to_vec())));
        }

        #[test]
        fn malformed_files_and_reserved_references_fail_without_disclosing_contents() {
            let (_tmp, path, r) = fixture();
            let store = FileStore::new(&path).unwrap();
            store.set(&r, Secret::new(CANARY.to_vec())).unwrap();
            for bytes in [
                b"private-canary-1234".as_slice(),
                br#"{"test-key":"private-canary-1234"}"#,
                br#"{"ai.meta.dev.credentials":"c2VjcmV0"}"#,
            ] {
                fs::write(path.join("secrets.json"), bytes).unwrap();
                assert_unavailable(store.get(&r));
                assert_unavailable(store.set(&r, Secret::new(CANARY.to_vec())));
                assert_unavailable(store.delete(&r));
            }
            assert_eq!(
                SecretRef::new("ai.meta.dev.credentials"),
                Err(VaultError::InvalidReference)
            );
            assert_eq!(format!("{store:?}"), "FileStore(<private>)");
            assert_eq!(
                format!("{:?}", Secret::new(CANARY.to_vec())),
                "Secret(<redacted>)"
            );
        }

        #[test]
        fn concurrent_independent_stores_preserve_all_updates() {
            let (_tmp, path, _r) = fixture();
            FileStore::new(&path).unwrap();
            let barrier = Arc::new(Barrier::new(8));
            let writers: Vec<_> = (0..8)
                .map(|writer| {
                    let path = path.clone();
                    let barrier = barrier.clone();
                    std::thread::spawn(move || {
                        let store = FileStore::new(path).unwrap();
                        barrier.wait();
                        for i in 0..16 {
                            let r = SecretRef::new(format!("writer-{writer}-{i}")).unwrap();
                            store.set(&r, Secret::new(CANARY.to_vec())).unwrap();
                            assert_eq!(store.get(&r).unwrap().unwrap().expose(), CANARY);
                            if i % 2 == 0 {
                                store.delete(&r).unwrap();
                            }
                        }
                    })
                })
                .collect();
            for writer in writers {
                writer.join().unwrap();
            }
            let store = FileStore::new(&path).unwrap();
            for writer in 0..8 {
                for i in 0..16 {
                    let r = SecretRef::new(format!("writer-{writer}-{i}")).unwrap();
                    assert_eq!(store.get(&r).unwrap().is_some(), i % 2 == 1);
                }
            }
            let entries: Entries =
                serde_json::from_slice(&fs::read(path.join("secrets.json")).unwrap()).unwrap();
            assert_eq!(entries.len(), 64);
        }

        #[test]
        fn process_writer() {
            let Some(path) = std::env::var_os("TASK12_VAULT_TEST_CHILD") else {
                return;
            };
            let id = std::env::var("TASK12_VAULT_TEST_WRITER").unwrap();
            let store = FileStore::new(path).unwrap();
            for i in 0..16 {
                let r = SecretRef::new(format!("process-{id}-{i}")).unwrap();
                store.set(&r, Secret::new(CANARY.to_vec())).unwrap();
            }
        }

        #[test]
        fn concurrent_processes_preserve_all_updates_and_atomic_json() {
            let (_tmp, path, r) = fixture();
            FileStore::new(&path)
                .unwrap()
                .set(&r, Secret::new(CANARY.to_vec()))
                .unwrap();
            let mut children: Vec<_> = (0..4)
                .map(|id| {
                    std::process::Command::new(std::env::current_exe().unwrap())
                        .args(["--exact", "file::unix::tests::process_writer"])
                        .env("TASK12_VAULT_TEST_CHILD", &path)
                        .env("TASK12_VAULT_TEST_WRITER", id.to_string())
                        .stdout(std::process::Stdio::null())
                        .spawn()
                        .unwrap()
                })
                .collect();
            // Read without the advisory lock: atomic replacement must expose complete JSON.
            loop {
                let bytes = fs::read(path.join("secrets.json")).unwrap();
                let _: Entries = serde_json::from_slice(&bytes).unwrap();
                if children
                    .iter_mut()
                    .all(|child| child.try_wait().unwrap().is_some())
                {
                    break;
                }
                std::thread::yield_now();
            }
            for mut child in children {
                assert!(child.wait().unwrap().success());
            }
            let store = FileStore::new(path).unwrap();
            for id in 0..4 {
                for i in 0..16 {
                    let r = SecretRef::new(format!("process-{id}-{i}")).unwrap();
                    assert_eq!(store.get(&r).unwrap().unwrap().expose(), CANARY);
                }
            }
        }
    }
}

// A chmod-based implementation cannot guarantee privacy on Windows. Fail closed.
#[cfg(not(unix))]
impl FileStore {
    pub fn new(_: impl AsRef<Path>) -> Result<Self, VaultError> {
        Err(VaultError::Unavailable)
    }
}
#[cfg(not(unix))]
impl SecretStore for FileStore {
    fn kind(&self) -> SecretStoreKind {
        SecretStoreKind::File
    }
    fn get(&self, _: &SecretRef) -> Result<Option<Secret>, VaultError> {
        Err(VaultError::Unavailable)
    }
    fn set(&self, _: &SecretRef, _: Secret) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }
    fn delete(&self, _: &SecretRef) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }
}
