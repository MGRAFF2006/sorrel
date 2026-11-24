//! Stored Hub CLI credentials (`sorrel login`).
//!
//! Credentials live in `<config dir>/sorrel/credentials.json`, keyed by the
//! normalized Hub URL so a token is only ever sent to the Hub that issued it.
//! The directory is created `0700` and the file is written `0600` through an
//! atomic temp-file rename. On Unix, a credentials file readable by group or
//! others is refused rather than used.
//!
//! `SORREL_CONFIG_DIR` replaces `<config dir>/sorrel` (useful for tests and
//! isolated agents). `SORREL_HUB_TOKEN` still overrides stored credentials.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Environment variable that replaces `<config dir>/sorrel`.
pub const CONFIG_DIR_ENV: &str = "SORREL_CONFIG_DIR";
const CREDENTIALS_FILE: &str = "credentials.json";
const FILE_VERSION: u32 = 1;

/// One Hub's stored CLI credential. `token` is the only secret field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredCredential {
    /// Opaque `srl_cli_…` bearer token. Never printed.
    pub token: String,
    /// Public token id (used by the Hub UI for revocation).
    pub token_id: String,
    /// Principal the Hub bound to this token.
    pub principal: Value,
    /// Label shown in the Hub UI.
    pub label: String,
    /// Absolute expiry as reported by the Hub (RFC 3339).
    pub expires_at: String,
}

impl StoredCredential {
    /// Public view for `auth status` and login output: never contains the token.
    #[must_use]
    pub fn public_json(&self, hub: &str) -> Value {
        serde_json::json!({
            "hub": hub,
            "principal": self.principal,
            "label": self.label,
            "tokenId": self.token_id,
            "expiresAt": self.expires_at,
        })
    }
}

/// The whole credentials file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialFile {
    version: u32,
    /// Normalized Hub URL → credential.
    pub hubs: BTreeMap<String, StoredCredential>,
}

/// Normalize a Hub URL for credential lookup.
///
/// Lower-cases scheme and host, drops default ports and trailing slashes, and
/// rejects credentials, queries and fragments. Two remotes share a credential
/// only when their normalized URLs are identical.
pub fn normalize_hub_url(raw: &str) -> io::Result<String> {
    let invalid = |reason: &str| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid Hub URL {raw:?}: {reason}"),
        )
    };
    let url = url::Url::parse(raw.trim()).map_err(|error| invalid(&error.to_string()))?;
    if url.scheme() != "https" && url.scheme() != "http" {
        return Err(invalid("use an http(s) URL"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid("credentials in the URL are not allowed"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(invalid("query strings and fragments are not allowed"));
    }
    let host = url.host_str().ok_or_else(|| invalid("missing host"))?;
    let port = url
        .port()
        .map(|port| format!(":{port}"))
        .unwrap_or_default();
    let path = url.path().trim_end_matches('/');
    Ok(format!("{}://{host}{port}{path}", url.scheme()))
}

/// Whether tokens may be sent to this normalized URL: HTTPS, or HTTP on loopback.
#[must_use]
pub fn is_secure_hub_url(normalized: &str) -> bool {
    let Ok(url) = url::Url::parse(normalized) else {
        return false;
    };
    match url.scheme() {
        "https" => true,
        "http" => match url.host() {
            Some(url::Host::Domain(domain)) => domain == "localhost",
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        },
        _ => false,
    }
}

/// `<config dir>/sorrel`, or `SORREL_CONFIG_DIR` when set.
pub fn config_dir() -> io::Result<PathBuf> {
    if let Some(dir) = std::env::var_os(CONFIG_DIR_ENV).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    let base = platform_config_dir().ok_or_else(|| {
        io::Error::other(format!(
            "cannot locate a config directory; set {CONFIG_DIR_ENV}"
        ))
    })?;
    Ok(base.join("sorrel"))
}

fn absolute_env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

#[cfg(windows)]
fn platform_config_dir() -> Option<PathBuf> {
    absolute_env("APPDATA")
}

#[cfg(target_os = "macos")]
fn platform_config_dir() -> Option<PathBuf> {
    absolute_env("HOME").map(|home| home.join("Library/Application Support"))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn platform_config_dir() -> Option<PathBuf> {
    absolute_env("XDG_CONFIG_HOME")
        .or_else(|| absolute_env("HOME").map(|home| home.join(".config")))
}

/// Path of the credentials file.
pub fn credentials_path() -> io::Result<PathBuf> {
    Ok(config_dir()?.join(CREDENTIALS_FILE))
}

/// Refuse symlinks and (on Unix) files readable or writable by group or others.
fn check_file_permissions(path: &Path, metadata: &fs::Metadata) -> io::Result<()> {
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "refusing credentials file {}: not a regular file",
                path.display()
            ),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "refusing credentials file {path}: it is accessible by group or others; run `chmod 600 {path}`, or delete it and sign in again",
                    path = path.display()
                ),
            ));
        }
    }
    Ok(())
}

/// Load the credentials file; a missing file is empty.
pub fn load() -> io::Result<CredentialFile> {
    load_from(&credentials_path()?)
}

fn load_from(path: &Path) -> io::Result<CredentialFile> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(CredentialFile {
                version: FILE_VERSION,
                hubs: BTreeMap::new(),
            })
        }
        Err(error) => return Err(error),
    };
    check_file_permissions(path, &metadata)?;
    let bytes = fs::read(path)?;
    let file: CredentialFile = serde_json::from_slice(&bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "credentials file {} is not valid; remove it and run `sorrel login` again",
                path.display()
            ),
        )
    })?;
    if file.version != FILE_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "credentials file {} has unsupported version {}",
                path.display(),
                file.version
            ),
        ));
    }
    Ok(file)
}

/// Atomically replace the credentials file (0700 directory, 0600 file).
pub fn save(file: &CredentialFile) -> io::Result<()> {
    save_to(&credentials_path()?, file)
}

fn save_to(path: &Path, file: &CredentialFile) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("credentials path has no parent directory"))?;
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    let mut payload = serde_json::to_vec_pretty(&CredentialFile {
        version: FILE_VERSION,
        hubs: file.hubs.clone(),
    })?;
    payload.push(b'\n');
    // tempfile creates the file 0600 on Unix; set it explicitly anyway.
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temp.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    temp.write_all(&payload)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|error| error.error)?;
    Ok(())
}

/// Stored credential for a Hub URL, matched by normalized URL only.
pub fn lookup(hub_url: &str) -> io::Result<Option<StoredCredential>> {
    let Ok(key) = normalize_hub_url(hub_url) else {
        // Not a URL we would ever have stored a credential for.
        return Ok(None);
    };
    Ok(load()?.hubs.remove(&key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_hub_urls_for_exact_matching() {
        for (raw, expected) in [
            (
                "https://Hub.Example.com/api/",
                "https://hub.example.com/api",
            ),
            (
                "https://hub.example.com:443/api",
                "https://hub.example.com/api",
            ),
            ("HTTP://127.0.0.1:3000/", "http://127.0.0.1:3000"),
            ("https://hub.example.com", "https://hub.example.com"),
            (
                "https://hub.example.com:8443/api//",
                "https://hub.example.com:8443/api",
            ),
        ] {
            assert_eq!(normalize_hub_url(raw).unwrap(), expected, "{raw}");
        }
        for raw in [
            "hub.example.com",
            "ftp://hub.example.com",
            "https://user:pw@hub.example.com",
            "https://hub.example.com/?x=1",
            "https://hub.example.com/#frag",
        ] {
            assert!(normalize_hub_url(raw).is_err(), "{raw}");
        }
        assert_ne!(
            normalize_hub_url("https://hub.example.com/api").unwrap(),
            normalize_hub_url("https://hub.example.com.evil.test/api").unwrap()
        );
    }

    #[test]
    fn only_https_or_loopback_http_may_carry_tokens() {
        for (url, secure) in [
            ("https://hub.example.com/api", true),
            ("http://127.0.0.1:3000", true),
            ("http://localhost:5181/api", true),
            ("http://[::1]:3000", true),
            ("http://hub.example.com/api", false),
            ("http://10.0.0.5:3000", false),
        ] {
            assert_eq!(is_secure_hub_url(url), secure, "{url}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn saves_private_files_atomically_and_refuses_shared_ones() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("sorrel");
        let path = dir.join(CREDENTIALS_FILE);
        let mut file = load_from(&path).unwrap();
        assert!(file.hubs.is_empty());
        file.hubs.insert(
            "https://hub.example.com/api".to_owned(),
            StoredCredential {
                token: "srl_cli_fixture".to_owned(),
                token_id: "id".to_owned(),
                principal: serde_json::json!({"type": "user", "id": "u"}),
                label: "laptop".to_owned(),
                expires_at: "2026-12-24T10:00:00.000Z".to_owned(),
            },
        );
        save_to(&path, &file).unwrap();
        assert_eq!(
            fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(load_from(&path).unwrap(), file);
        assert_eq!(
            fs::read_dir(&dir).unwrap().count(),
            1,
            "no temp files are left behind"
        );

        for mode in [0o640, 0o604, 0o660] {
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            let error = load_from(&path).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied, "{mode:o}");
            assert!(error.to_string().contains("chmod 600"));
        }
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        save_to(&path, &file).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600,
            "rewrites replace a shared file with a private one"
        );

        let link = dir.join("link.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert_eq!(
            load_from(&link).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }
}
