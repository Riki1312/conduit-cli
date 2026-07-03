use crate::config::{ConfigError, validate_secret_name};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

const SECRET_DIR_ENV: &str = "CONDUIT_SECRET_DIR";
const XDG_STATE_HOME_ENV: &str = "XDG_STATE_HOME";
const HOME_ENV: &str = "HOME";

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SecretStoreError {
    pub(crate) kind: SecretStoreErrorKind,
    pub(crate) message: String,
}

impl SecretStoreError {
    fn new(kind: SecretStoreErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SecretStoreErrorKind {
    InvalidName,
    Internal,
}

pub(crate) fn read_secret(name: &str) -> Result<Option<String>, SecretStoreError> {
    read_secret_from_dir(&secret_dir(), name)
}

pub(crate) fn write_secret(name: &str, value: &str) -> Result<(), SecretStoreError> {
    write_secret_to_dir(&secret_dir(), name, value)
}

pub(crate) fn delete_secret(name: &str) -> Result<bool, SecretStoreError> {
    delete_secret_from_dir(&secret_dir(), name)
}

fn read_secret_from_dir(root: &Path, name: &str) -> Result<Option<String>, SecretStoreError> {
    let path = secret_path(root, name)?;
    match fs::read_to_string(&path) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(SecretStoreError::new(
            SecretStoreErrorKind::Internal,
            format!("failed to read secret `{name}`: {error}"),
        )),
    }
}

fn write_secret_to_dir(root: &Path, name: &str, value: &str) -> Result<(), SecretStoreError> {
    let path = secret_path(root, name)?;
    create_secret_dir(root)?;
    if let Some(parent) = path.parent() {
        create_secret_dir(parent)?;
    }

    fs::write(&path, value).map_err(|error| {
        SecretStoreError::new(
            SecretStoreErrorKind::Internal,
            format!("failed to write secret `{name}`: {error}"),
        )
    })?;
    set_secret_file_permissions(&path)?;

    Ok(())
}

fn delete_secret_from_dir(root: &Path, name: &str) -> Result<bool, SecretStoreError> {
    let path = secret_path(root, name)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(SecretStoreError::new(
            SecretStoreErrorKind::Internal,
            format!("failed to delete secret `{name}`: {error}"),
        )),
    }
}

fn secret_path(root: &Path, name: &str) -> Result<PathBuf, SecretStoreError> {
    validate_secret_name("secret", name).map_err(secret_name_error)?;
    Ok(root.join(name))
}

fn secret_name_error(error: ConfigError) -> SecretStoreError {
    SecretStoreError::new(SecretStoreErrorKind::InvalidName, error.message)
}

fn create_secret_dir(path: &Path) -> Result<(), SecretStoreError> {
    fs::create_dir_all(path).map_err(|error| {
        SecretStoreError::new(
            SecretStoreErrorKind::Internal,
            format!(
                "failed to create secret directory {}: {error}",
                path.display()
            ),
        )
    })?;
    set_secret_dir_permissions(path)
}

#[cfg(unix)]
fn set_secret_dir_permissions(path: &Path) -> Result<(), SecretStoreError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
        SecretStoreError::new(
            SecretStoreErrorKind::Internal,
            format!(
                "failed to set secret directory permissions on {}: {error}",
                path.display()
            ),
        )
    })
}

#[cfg(not(unix))]
fn set_secret_dir_permissions(_path: &Path) -> Result<(), SecretStoreError> {
    Ok(())
}

#[cfg(unix)]
fn set_secret_file_permissions(path: &Path) -> Result<(), SecretStoreError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|error| {
        SecretStoreError::new(
            SecretStoreErrorKind::Internal,
            format!(
                "failed to set secret file permissions on {}: {error}",
                path.display()
            ),
        )
    })
}

#[cfg(not(unix))]
fn set_secret_file_permissions(_path: &Path) -> Result<(), SecretStoreError> {
    Ok(())
}

fn secret_dir() -> PathBuf {
    secret_dir_from_env(
        env::var_os(SECRET_DIR_ENV),
        env::var_os(XDG_STATE_HOME_ENV),
        env::var_os(HOME_ENV),
    )
}

fn secret_dir_from_env(
    secret_dir: Option<OsString>,
    xdg_state_home: Option<OsString>,
    home: Option<OsString>,
) -> PathBuf {
    if let Some(path) = secret_dir {
        return PathBuf::from(path);
    }

    if let Some(path) = xdg_state_home {
        return PathBuf::from(path).join("conduit/secrets");
    }

    if let Some(path) = home {
        return PathBuf::from(path).join(".local/state/conduit/secrets");
    }

    PathBuf::from(".conduit/state/secrets")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn resolves_user_secret_directory() {
        assert_eq!(
            secret_dir_from_env(
                Some("/tmp/conduit-secrets-test".into()),
                Some("/tmp/xdg-state".into()),
                Some("/tmp/home".into())
            ),
            PathBuf::from("/tmp/conduit-secrets-test")
        );
        assert_eq!(
            secret_dir_from_env(
                None,
                Some("/tmp/xdg-state".into()),
                Some("/tmp/home".into())
            ),
            PathBuf::from("/tmp/xdg-state/conduit/secrets")
        );
        assert_eq!(
            secret_dir_from_env(None, None, Some("/tmp/home".into())),
            PathBuf::from("/tmp/home/.local/state/conduit/secrets")
        );
    }

    #[test]
    fn rejects_invalid_secret_names() {
        let error = secret_path(Path::new("/tmp"), "../cookie").unwrap_err();

        assert_eq!(error.kind, SecretStoreErrorKind::InvalidName);
    }

    #[test]
    fn writes_reads_and_deletes_secret_values() {
        let root = test_dir("secret-store");

        write_secret_to_dir(&root, "company-logs/staging/cookie", "cookie=value").unwrap();

        assert_eq!(
            read_secret_from_dir(&root, "company-logs/staging/cookie")
                .unwrap()
                .as_deref(),
            Some("cookie=value")
        );
        assert!(delete_secret_from_dir(&root, "company-logs/staging/cookie").unwrap());
        assert_eq!(
            read_secret_from_dir(&root, "company-logs/staging/cookie").unwrap(),
            None
        );

        fs::remove_dir_all(root).unwrap();
    }

    fn test_dir(name: &str) -> PathBuf {
        let mut path = env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("conduit-{name}-{nanos}"));
        path
    }
}
