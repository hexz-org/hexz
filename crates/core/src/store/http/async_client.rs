//! Asynchronous HTTP Storage Backend.

#[cfg(feature = "async-http")]
use crate::store::StorageBackend;
#[cfg(feature = "async-http")]
use crate::store::utils::validate_url;
#[cfg(feature = "async-http")]
use bytes::Bytes;
#[cfg(feature = "async-http")]
use reqwest::Client;
#[cfg(feature = "async-http")]
use std::io::{Error, ErrorKind};
#[cfg(feature = "async-http")]
use std::sync::Arc;
#[cfg(feature = "async-http")]
use strata_common::{Result, StrataError};
#[cfg(feature = "async-http")]
use tokio::runtime::Runtime;

#[cfg(feature = "async-http")]
#[derive(Debug)]
pub struct AsyncHttpBackend {
    url: String,
    client: Client,
    len: u64,
    runtime: Arc<Runtime>,
}

#[cfg(feature = "async-http")]
impl AsyncHttpBackend {
    pub fn new(url: String, allow_restricted: bool) -> Result<Self> {
        let safe_url = validate_url(&url, allow_restricted)?;

        let runtime = Runtime::new().map_err(|e| StrataError::Io(Error::other(e)))?;

        let client = Client::builder()
            .build()
            .map_err(|e| StrataError::Io(Error::other(e)))?;

        let len = runtime.block_on(async {
            let resp = client
                .head(&safe_url)
                .send()
                .await
                .map_err(|e| StrataError::Io(Error::other(e)))?;

            if !resp.status().is_success() {
                return Err(StrataError::Io(Error::other(format!(
                    "HTTP error: {}",
                    resp.status()
                ))));
            }

            resp.headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|val| val.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or_else(|| {
                    StrataError::Io(Error::new(
                        ErrorKind::InvalidData,
                        "Missing Content-Length header",
                    ))
                })
        })?;

        Ok(Self {
            url: safe_url,
            client,
            len,
            runtime: Arc::new(runtime),
        })
    }
}

#[cfg(feature = "async-http")]
impl StorageBackend for AsyncHttpBackend {
    fn read_exact(&self, offset: u64, len: usize) -> Result<Bytes> {
        let end = offset + len as u64 - 1;
        let range_header = format!("bytes={}-{}", offset, end);

        self.runtime.block_on(async {
            let resp = self
                .client
                .get(&self.url)
                .header("Range", range_header)
                .send()
                .await
                .map_err(|e| StrataError::Io(Error::other(e)))?;

            if !resp.status().is_success() {
                return Err(StrataError::Io(Error::other(format!(
                    "HTTP error: {}",
                    resp.status()
                ))));
            }

            let bytes = resp
                .bytes()
                .await
                .map_err(|e| StrataError::Io(Error::other(e)))?;

            if bytes.len() != len {
                return Err(StrataError::Io(Error::new(
                    ErrorKind::UnexpectedEof,
                    format!("Expected {} bytes, got {}", len, bytes.len()),
                )));
            }

            Ok(bytes)
        })
    }

    fn len(&self) -> u64 {
        self.len
    }
}
