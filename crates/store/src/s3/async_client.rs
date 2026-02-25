//! S3 storage backend with embedded Tokio runtime.

use crate::runtime::global_handle;
use bytes::Bytes;
use hexz_common::{Error, Result};
use hexz_core::store::StorageBackend;
use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::region::Region;
use std::io::{Error as IoError, ErrorKind};
use std::str::FromStr;
use tokio::runtime::Handle;

/// S3 storage backend with embedded Tokio runtime.
#[derive(Debug)]
pub struct S3Backend {
    bucket: Box<Bucket>,
    key: String,
    len: u64,
    handle: Handle,
}

impl S3Backend {
    /// Creates an S3 backend, verifies the object exists, and caches its length.
    pub fn new(
        bucket_name: String,
        key: String,
        region_name: String,
        endpoint: Option<String>,
    ) -> Result<Self> {
        let handle = global_handle();

        let region = if let Some(ep) = endpoint {
            Region::Custom {
                region: region_name,
                endpoint: ep,
            }
        } else {
            Region::from_str(&region_name).map_err(|e| {
                Error::Io(IoError::new(
                    ErrorKind::InvalidInput,
                    format!("Invalid region: {}", e),
                ))
            })?
        };

        let credentials = Credentials::default().map_err(|e| {
            Error::Io(IoError::new(
                ErrorKind::PermissionDenied,
                format!("Missing credentials: {}", e),
            ))
        })?;

        let bucket = Bucket::new(&bucket_name, region, credentials)
            .map_err(|e| Error::Io(IoError::other(format!("Bucket error: {}", e))))?
            .with_path_style();

        let (head, code) = handle.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(30), bucket.head_object(&key))
                .await
                .map_err(|_| {
                    Error::Io(IoError::new(
                        ErrorKind::TimedOut,
                        "S3 connection timeout after 30 seconds",
                    ))
                })?
                .map_err(|e| Error::Io(IoError::other(format!("S3 Head error: {}", e))))
        })?;

        if code != 200 {
            return Err(Error::Io(IoError::new(
                ErrorKind::NotFound,
                format!("S3 object not found or error: {}", code),
            )));
        }

        let len = head.content_length.ok_or_else(|| {
            Error::Io(IoError::new(
                ErrorKind::InvalidData,
                "Missing Content-Length",
            ))
        })?;

        if len < 0 {
            return Err(Error::Io(IoError::new(
                ErrorKind::InvalidData,
                "Negative Content-Length",
            )));
        }

        Ok(Self {
            bucket: Box::new(bucket),
            key,
            len: len as u64,
            handle,
        })
    }
}

impl StorageBackend for S3Backend {
    fn read_exact(&self, offset: u64, len: usize) -> Result<Bytes> {
        if len == 0 {
            return Ok(Bytes::new());
        }
        let end = offset + len as u64 - 1;

        self.handle.block_on(async {
            let response_data = tokio::time::timeout(
                std::time::Duration::from_secs(60),
                self.bucket.get_object_range(&self.key, offset, Some(end)),
            )
            .await
            .map_err(|_| {
                Error::Io(IoError::new(
                    ErrorKind::TimedOut,
                    "S3 read timeout after 60 seconds",
                ))
            })?
            .map_err(|e| Error::Io(IoError::other(format!("S3 Read error: {}", e))))?;

            let code = response_data.status_code();
            if code != 200 && code != 206 {
                return Err(Error::Io(IoError::other(format!(
                    "S3 error code: {}",
                    code
                ))));
            }

            let data = response_data.as_slice();
            if data.len() != len {
                return Err(Error::Io(IoError::new(
                    ErrorKind::UnexpectedEof,
                    format!("Expected {} bytes, got {}", len, data.len()),
                )));
            }

            Ok(Bytes::copy_from_slice(data))
        })
    }

    fn len(&self) -> u64 {
        self.len
    }
}
