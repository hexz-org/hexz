//! Asynchronous S3 Storage Backend.
//!
//! Provides a `StorageBackend` implementation that uses the async `rust-s3` client
//! wrapped in a Tokio runtime. This is useful when running in contexts where
//! blocking I/O is undesirable or to leverage the ecosystem's async networking stack.

#[cfg(feature = "s3")]
use crate::store::StorageBackend;
#[cfg(feature = "s3")]
use bytes::Bytes;
#[cfg(feature = "s3")]
use s3::bucket::Bucket;
#[cfg(feature = "s3")]
use s3::creds::Credentials;
#[cfg(feature = "s3")]
use s3::region::Region;
#[cfg(feature = "s3")]
use std::io::{Error, ErrorKind};
#[cfg(feature = "s3")]
use std::str::FromStr;
#[cfg(feature = "s3")]
use std::sync::Arc;
#[cfg(feature = "s3")]
use strata_common::{Result, StrataError};
#[cfg(feature = "s3")]
use tokio::runtime::Runtime;

#[cfg(feature = "s3")]
#[derive(Debug)]
pub struct AsyncS3Backend {
    bucket: Box<Bucket>,
    key: String,
    len: u64,
    runtime: Arc<Runtime>,
}

#[cfg(feature = "s3")]
impl AsyncS3Backend {
    pub fn new(
        bucket_name: String,
        key: String,
        region_name: String,
        endpoint: Option<String>,
    ) -> Result<Self> {
        let runtime = Runtime::new().map_err(StrataError::Io)?;

        let region = if let Some(ep) = endpoint {
            Region::Custom {
                region: region_name,
                endpoint: ep,
            }
        } else {
            Region::from_str(&region_name).map_err(|e| {
                StrataError::Io(Error::new(
                    ErrorKind::InvalidInput,
                    format!("Invalid region: {}", e),
                ))
            })?
        };

        let credentials = Credentials::default().map_err(|e| {
            StrataError::Io(Error::new(
                ErrorKind::PermissionDenied,
                format!("Missing credentials: {}", e),
            ))
        })?;

        let bucket = Bucket::new(&bucket_name, region, credentials)
            .map_err(|e| StrataError::Io(Error::other(format!("Bucket error: {}", e))))?
            .with_path_style();

        // Perform HEAD request to get size and validate access
        let (head, code) = runtime
            .block_on(async { bucket.head_object(&key).await })
            .map_err(|e| StrataError::Io(Error::other(format!("S3 Head error: {}", e))))?;

        if code != 200 {
            return Err(StrataError::Io(Error::new(
                ErrorKind::NotFound,
                format!("S3 object not found or error: {}", code),
            )));
        }

        let len = head.content_length.ok_or_else(|| {
            StrataError::Io(Error::new(ErrorKind::InvalidData, "Missing Content-Length"))
        })?;

        if len < 0 {
            return Err(StrataError::Io(Error::new(
                ErrorKind::InvalidData,
                "Negative Content-Length",
            )));
        }

        Ok(Self {
            bucket: Box::new(bucket),
            key,
            len: len as u64,
            runtime: Arc::new(runtime),
        })
    }
}

#[cfg(feature = "s3")]
impl StorageBackend for AsyncS3Backend {
    fn read_exact(&self, offset: u64, len: usize) -> Result<Bytes> {
        let end = offset + len as u64 - 1;

        self.runtime.block_on(async {
            let response_data = self
                .bucket
                .get_object_range(&self.key, offset, Some(end))
                .await
                .map_err(|e| StrataError::Io(Error::other(format!("S3 Read error: {}", e))))?;

            let code = response_data.status_code();
            if code != 200 && code != 206 {
                return Err(StrataError::Io(Error::other(format!(
                    "S3 error code: {}",
                    code
                ))));
            }

            let data = response_data.as_slice();

            if data.len() != len {
                return Err(StrataError::Io(Error::new(
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
