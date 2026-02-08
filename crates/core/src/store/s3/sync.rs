//! Amazon S3 Storage Backend.
//!
//! This module implements the `StorageBackend` interface for objects stored in
//! Amazon S3 (Simple Storage Service) or compatible object storage systems.
//! It enables direct random access to snapshot images stored in the cloud
//! without requiring a full download, making it ideal for cloud-native deployments.

use crate::store::StorageBackend;
use bytes::Bytes;
use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::region::Region;
use std::io::{Error, ErrorKind};
use std::str::FromStr;
use strata_common::{Result, StrataError};

/// A storage backend for accessing objects in an S3 bucket.
///
/// This struct encapsulates the necessary configuration (bucket, credentials, region)
/// to authenticate and perform operations against the S3 API. It uses blocking
/// network calls to satisfy the `StorageBackend` contract.
#[derive(Debug)]
pub struct S3Backend {
    /// The S3 bucket client instance used for API operations.
    bucket: Bucket,
    /// The object key (path) identifying the snapshot within the bucket.
    key: String,
    /// The total size of the S3 object in bytes, cached at initialization.
    len: u64,
}

impl S3Backend {
    /// Initializes a new S3 storage backend.
    ///
    /// This constructor sets up the S3 client, authenticates using available credentials
    /// (environment variables, AWS config files, or IAM instance profiles), and
    /// verifies access to the specified object via a HEAD request.
    ///
    /// # Arguments
    ///
    /// * `bucket_name` - The name of the S3 bucket.
    /// * `key` - The key (path) of the snapshot object.
    /// * `region_name` - The AWS region string (e.g., "us-east-1").
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the initialized `S3Backend` on success.
    /// Returns an error if the region is invalid, credentials are missing, or the
    /// object cannot be accessed.
    pub fn new(bucket_name: String, key: String, region_name: String) -> Result<Self> {
        let region = Region::from_str(&region_name).map_err(|e| {
            StrataError::Io(Error::new(
                ErrorKind::InvalidInput,
                format!("Invalid region: {}", e),
            ))
        })?;

        let credentials = Credentials::default().map_err(|e| {
            StrataError::Io(Error::new(
                ErrorKind::PermissionDenied,
                format!("Missing credentials: {}", e),
            ))
        })?;

        let bucket = Bucket::new(&bucket_name, region, credentials)
            .map_err(|e| StrataError::Io(Error::other(format!("Bucket error: {}", e))))?
            .with_path_style();

        let (head, code) = bucket
            .head_object_blocking(&key)
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
            bucket,
            key,
            len: len as u64,
        })
    }
}

impl StorageBackend for S3Backend {
    /// Fetches a byte range from the S3 object using the GetObject API.
    ///
    /// This method constructs a `Range` header request to download only the
    /// specified segment of the object. This minimizes bandwidth and latency
    /// when accessing small parts of a large snapshot.
    ///
    /// # Arguments
    ///
    /// * `offset` - The byte offset from the start of the object.
    /// * `len` - The number of bytes to read.
    ///
    /// # Returns
    ///
    /// Returns a `Bytes` buffer containing the object data. Returns an error if the
    /// S3 API returns a non-success status code or incomplete data.
    fn read_exact(&self, offset: u64, len: usize) -> Result<Bytes> {
        let end = offset + len as u64 - 1;

        let response_data = self
            .bucket
            .get_object_range_blocking(&self.key, offset, Some(end))
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
    }

    /// Returns the cached size of the S3 object.
    ///
    /// This value is obtained from the `Content-Length` header during the
    /// initial HEAD request and is used to define the file boundaries.
    fn len(&self) -> u64 {
        self.len
    }
}
