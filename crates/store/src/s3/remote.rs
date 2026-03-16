//! S3 implementation of [`RemoteTransport`](crate::remote::RemoteTransport).

use crate::remote::{RemoteArchiveInfo, RemoteTransport};
use crate::runtime::global_handle;
use hexz_common::{Error, Result};
use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::region::Region;
use std::io::{Error as IoError, ErrorKind};
use std::path::Path;
use tokio::runtime::Handle;

/// S3-backed remote transport for push/pull operations.
#[derive(Debug)]
pub struct S3Remote {
    bucket: Box<Bucket>,
    prefix: String,
    handle: Handle,
}

/// Parse an `s3://bucket/prefix` URL into `(bucket_name, prefix)`.
///
/// The prefix is normalised to end with `/` (or be empty for bucket root).
fn parse_s3_url(url: &str) -> Result<(String, String)> {
    let rest = url.strip_prefix("s3://").ok_or_else(|| {
        Error::Io(IoError::new(
            ErrorKind::InvalidInput,
            format!("Not an S3 URL: {url}"),
        ))
    })?;

    let (bucket, prefix) = match rest.find('/') {
        Some(idx) => {
            let bucket = &rest[..idx];
            let mut prefix = rest[idx + 1..].to_string();
            // Normalise: strip trailing slash then re-add, so "a/b" and "a/b/" both become "a/b/"
            if !prefix.is_empty() {
                prefix = prefix.trim_end_matches('/').to_string();
                prefix.push('/');
            }
            (bucket.to_string(), prefix)
        }
        None => (rest.to_string(), String::new()),
    };

    if bucket.is_empty() {
        return Err(Error::Io(IoError::new(
            ErrorKind::InvalidInput,
            "S3 URL has empty bucket name",
        )));
    }

    Ok((bucket, prefix))
}

impl S3Remote {
    /// Connect to an S3 remote given an `s3://bucket/prefix` URL.
    ///
    /// Uses `AWS_REGION` (default `us-east-1`), `AWS_ENDPOINT` (optional, for
    /// MinIO etc.), and standard `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`
    /// credentials.
    pub fn connect(url: &str) -> Result<Self> {
        let handle = global_handle().map_err(Error::Io)?;
        let (bucket_name, prefix) = parse_s3_url(url)?;

        let region_name =
            std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string());

        let region = if let Ok(endpoint) = std::env::var("AWS_ENDPOINT") {
            Region::Custom {
                region: region_name,
                endpoint,
            }
        } else {
            region_name
                .parse::<Region>()
                .map_err(|e| Error::Io(IoError::new(ErrorKind::InvalidInput, format!("Invalid region: {e}"))))?
        };

        let credentials = Credentials::default().map_err(|e| {
            Error::Io(IoError::new(
                ErrorKind::PermissionDenied,
                format!("Missing credentials: {e}"),
            ))
        })?;

        let bucket = Bucket::new(&bucket_name, region, credentials)
            .map_err(|e| Error::Io(IoError::other(format!("Bucket error: {e}"))))?
            .with_path_style();

        Ok(Self {
            bucket: Box::new(bucket),
            prefix,
            handle,
        })
    }

    /// Build the full S3 key for an archive name.
    fn key(&self, archive_name: &str) -> String {
        format!("{}{}", self.prefix, archive_name)
    }
}

impl RemoteTransport for S3Remote {
    fn list_archives(&self) -> Result<Vec<RemoteArchiveInfo>> {
        tokio::task::block_in_place(|| {
            self.handle.block_on(async {
                let results = self
                    .bucket
                    .list(self.prefix.clone(), None)
                    .await
                    .map_err(|e| Error::Io(IoError::other(format!("S3 list error: {e}"))))?;

                let mut archives = Vec::new();
                for list in &results {
                    for obj in &list.contents {
                        let name = obj.key.strip_prefix(&self.prefix).unwrap_or(&obj.key);
                        if name.ends_with(".hxz") && !name.contains('/') {
                            archives.push(RemoteArchiveInfo {
                                name: name.to_string(),
                                size: obj.size,
                            });
                        }
                    }
                }
                Ok(archives)
            })
        })
    }

    fn upload(&self, local_path: &Path, remote_name: &str) -> Result<()> {
        let data = std::fs::read(local_path).map_err(Error::Io)?;
        let key = self.key(remote_name);

        tokio::task::block_in_place(|| {
            self.handle.block_on(async {
                let response = self
                    .bucket
                    .put_object(&key, &data)
                    .await
                    .map_err(|e| Error::Io(IoError::other(format!("S3 upload error: {e}"))))?;

                let code = response.status_code();
                if code != 200 {
                    return Err(Error::Io(IoError::other(format!(
                        "S3 upload failed with status {code}"
                    ))));
                }
                Ok(())
            })
        })
    }

    fn download(&self, remote_name: &str, local_path: &Path) -> Result<()> {
        let key = self.key(remote_name);

        tokio::task::block_in_place(|| {
            self.handle.block_on(async {
                let response = self
                    .bucket
                    .get_object(&key)
                    .await
                    .map_err(|e| Error::Io(IoError::other(format!("S3 download error: {e}"))))?;

                let code = response.status_code();
                if code != 200 {
                    return Err(Error::Io(IoError::other(format!(
                        "S3 download failed with status {code}"
                    ))));
                }

                std::fs::write(local_path, response.bytes()).map_err(Error::Io)?;
                Ok(())
            })
        })
    }

    fn exists(&self, remote_name: &str) -> Result<bool> {
        let key = self.key(remote_name);

        tokio::task::block_in_place(|| {
            self.handle.block_on(async {
                let (_, code) = self
                    .bucket
                    .head_object(&key)
                    .await
                    .map_err(|e| Error::Io(IoError::other(format!("S3 head error: {e}"))))?;

                Ok(code == 200)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_s3_url_bucket_only() {
        let (bucket, prefix) = parse_s3_url("s3://my-bucket").unwrap();
        assert_eq!(bucket, "my-bucket");
        assert_eq!(prefix, "");
    }

    #[test]
    fn test_parse_s3_url_with_prefix() {
        let (bucket, prefix) = parse_s3_url("s3://my-bucket/archives/v1").unwrap();
        assert_eq!(bucket, "my-bucket");
        assert_eq!(prefix, "archives/v1/");
    }

    #[test]
    fn test_parse_s3_url_with_trailing_slash() {
        let (bucket, prefix) = parse_s3_url("s3://my-bucket/archives/").unwrap();
        assert_eq!(bucket, "my-bucket");
        assert_eq!(prefix, "archives/");
    }

    #[test]
    fn test_parse_s3_url_empty_prefix() {
        let (bucket, prefix) = parse_s3_url("s3://my-bucket/").unwrap();
        assert_eq!(bucket, "my-bucket");
        assert_eq!(prefix, "");
    }

    #[test]
    fn test_parse_s3_url_not_s3() {
        assert!(parse_s3_url("http://example.com").is_err());
    }

    #[test]
    fn test_parse_s3_url_empty_bucket() {
        assert!(parse_s3_url("s3://").is_err());
    }
}
