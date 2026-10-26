use crate::MuseError;
use macroscope_vault::Secret;
use std::{collections::BTreeMap, ffi::OsString, path::Path};
use tokio::process::Command;

pub const SAFE_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialRoute {
    ApiKey,
    PlanSignIn,
}
/// Never derives Debug: even the environment's credential slot is redacted.
pub struct ChildEnv {
    values: BTreeMap<OsString, OsString>,
    key: Option<Secret>,
}
impl std::fmt::Debug for ChildEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChildEnv(<allowlisted; secrets redacted>)")
    }
}
impl ChildEnv {
    pub fn for_route(
        route: CredentialRoute,
        private_home: &Path,
        real_home: &Path,
        key: Option<Secret>,
    ) -> Result<Self, MuseError> {
        let mut values = BTreeMap::new();
        values.insert("PATH".into(), SAFE_PATH.into());
        for name in ["LANG", "TMPDIR", "USER"] {
            if let Some(v) = std::env::var_os(name) {
                values.insert(name.into(), v);
            }
        }
        values.insert("MUSE_NO_AUTO_UPDATE".into(), "1".into());
        values.insert("MUSE_LOGIN".into(), "0".into());
        match route {
            CredentialRoute::ApiKey => {
                if key.as_ref().is_none_or(|k| k.expose().is_empty()) {
                    return Err(MuseError::KeyUnavailable);
                }
                std::fs::create_dir_all(private_home)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(private_home, std::fs::Permissions::from_mode(0o700))?;
                }
                values.insert("HOME".into(), private_home.as_os_str().to_owned());
                for (name, dir) in [
                    ("XDG_CONFIG_HOME", "config"),
                    ("XDG_DATA_HOME", "data"),
                    ("XDG_STATE_HOME", "state"),
                    ("XDG_CACHE_HOME", "cache"),
                ] {
                    let path = private_home.join(dir);
                    std::fs::create_dir_all(&path)?;
                    values.insert(name.into(), path.into_os_string());
                }
            }
            CredentialRoute::PlanSignIn => {
                values.insert("HOME".into(), real_home.as_os_str().to_owned());
            }
        }
        Ok(Self {
            values,
            key: if route == CredentialRoute::ApiKey {
                key
            } else {
                None
            },
        })
    }
    pub(crate) fn apply(&self, cmd: &mut Command, env_key: bool) -> Result<(), MuseError> {
        cmd.env_clear().envs(&self.values);
        if env_key {
            if let Some(k) = &self.key {
                #[cfg(unix)]
                {
                    use std::os::unix::ffi::OsStrExt;
                    cmd.env("META_API_KEY", std::ffi::OsStr::from_bytes(k.expose()));
                }
                #[cfg(not(unix))]
                {
                    cmd.env(
                        "META_API_KEY",
                        std::str::from_utf8(k.expose()).map_err(|_| MuseError::KeyUnavailable)?,
                    );
                }
            }
        }
        Ok(())
    }
    pub(crate) fn key(&self) -> Option<&Secret> {
        self.key.as_ref()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn canary_parent_env_never_reaches_child_and_key_never_argv() {
        std::env::set_var("MACROSCOPE_PARENT_CANARY", "bogus-parent-secret");
        let temp = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let real = temp.path().join("real");
        let env = ChildEnv::for_route(
            CredentialRoute::ApiKey,
            &temp.path().join("private"),
            &real,
            Some(Secret::new(b"bogus-key-canary".to_vec())),
        )
        .unwrap();
        let mut cmd = Command::new("/usr/bin/env");
        env.apply(&mut cmd, true).unwrap();
        assert!(!format!("{:?}", cmd.as_std().get_args()).contains("bogus-key-canary"));
        assert!(!format!("{env:?}").contains("bogus-key-canary"));
        let out = cmd.output().await.unwrap();
        let vars = String::from_utf8(out.stdout).unwrap();
        assert!(!vars.contains("MACROSCOPE_PARENT_CANARY"));
        assert!(!vars.contains("bogus-parent-secret"));
        assert!(vars.contains("META_API_KEY=bogus-key-canary"));
        let plan = ChildEnv::for_route(
            CredentialRoute::PlanSignIn,
            temp.path(),
            &real,
            Some(Secret::new(b"bogus-key-canary".to_vec())),
        )
        .unwrap();
        let mut cmd = Command::new("/usr/bin/env");
        plan.apply(&mut cmd, true).unwrap();
        let vars = String::from_utf8(cmd.output().await.unwrap().stdout).unwrap();
        assert!(!vars.contains("META_API_KEY"));
        assert!(!vars.contains("XDG_"));
        assert!(vars.contains(real.to_str().unwrap()));
        std::env::remove_var("MACROSCOPE_PARENT_CANARY");
    }
}
