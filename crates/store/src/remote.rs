//! Transport trait for remote archive storage.
//!
//! Abstracts over S3, HTTP, or other remote backends so that CLI push/pull
//! logic is transport-agnostic.

use hexz_common::{Error, Result};
use std::io::{Error as IoError, ErrorKind};
use std::path::Path;

/// Metadata about an archive on a remote.
#[derive(Debug, Clone)]
pub struct RemoteArchiveInfo {
    /// File name (e.g. `"v2.hxz"`).
    pub name: String,
    /// Size in bytes.
    pub size: u64,
}

/// Transport interface for uploading/downloading archives to/from a remote.
pub trait RemoteTransport: Send + Sync {
    /// List all `.hxz` archives on the remote.
    fn list_archives(&self) -> Result<Vec<RemoteArchiveInfo>>;

    /// Upload a local file to the remote under `remote_name`.
    fn upload(&self, local_path: &Path, remote_name: &str) -> Result<()>;

    /// Download an archive from the remote to a local path.
    fn download(&self, remote_name: &str, local_path: &Path) -> Result<()>;

    /// Check whether an archive exists on the remote.
    fn exists(&self, remote_name: &str) -> Result<bool>;
}

/// Connect to a remote given its URL, returning the appropriate transport.
///
/// Currently supports `s3://bucket/prefix` URLs. HTTP support is planned.
pub fn connect(url: &str) -> Result<Box<dyn RemoteTransport>> {
    if url.starts_with("s3://") {
        #[cfg(feature = "s3")]
        {
            let remote = crate::s3::remote::S3Remote::connect(url)?;
            return Ok(Box::new(remote));
        }
        #[cfg(not(feature = "s3"))]
        {
            return Err(Error::Io(IoError::new(
                ErrorKind::Unsupported,
                "S3 support not compiled in (missing feature \"s3\")",
            )));
        }
    }

    if url.starts_with("http://") || url.starts_with("https://") {
        return Err(Error::Io(IoError::new(
            ErrorKind::Unsupported,
            "HTTP remotes are not yet supported",
        )));
    }

    Err(Error::Io(IoError::new(
        ErrorKind::InvalidInput,
        format!("Unsupported remote URL scheme: {url}"),
    )))
}
