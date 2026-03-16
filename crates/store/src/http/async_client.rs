//! HTTP storage backend with embedded Tokio runtime.

use crate::runtime::global_handle;
use crate::utils::validate_url;
use bytes::Bytes;
use hexz_common::{Error, Result};
use hexz_core::store::StorageBackend;
use reqwest::Client;
use reqwest::header::HeaderMap;
use reqwest::redirect::Policy;
use std::io::{Error as IoError, ErrorKind};
use tokio::runtime::Handle;

const MAX_REDIRECTS: usize = 10;

async fn send_with_redirects(
    client: &Client,
    method: reqwest::Method,
    url: &str,
    extra_headers: HeaderMap,
) -> Result<reqwest::Response> {
    let mut current_url = url.to_string();
    let mut current_method = method;

    for _ in 0..=MAX_REDIRECTS {
        let resp = client
            .request(current_method.clone(), &current_url)
            .headers(extra_headers.clone())
            .send()
            .await
            .map_err(|e| Error::Io(IoError::other(e)))?;

        if !resp.status().is_redirection() {
            if !resp.status().is_success() {
                return Err(Error::Io(IoError::other(format!(
                    "HTTP {} for {}",
                    resp.status(),
                    current_url
                ))));
            }
            return Ok(resp);
        }

        let location = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                Error::Io(IoError::new(
                    ErrorKind::InvalidData,
                    "Redirect without Location header",
                ))
            })?
            .to_string();

        if resp.status().as_u16() == 303 {
            current_method = reqwest::Method::GET;
        }
        current_url = location;
    }

    Err(Error::Io(IoError::other(format!(
        "Too many redirects (>{MAX_REDIRECTS})"
    ))))
}

/// HTTP storage backend with embedded Tokio runtime.
#[derive(Debug)]
pub struct HttpBackend {
    url: String,
    client: Client,
    len: u64,
    handle: Handle,
}

impl HttpBackend {
    /// Creates an HTTP backend, validates the URL, and fetches file length via HEAD.
    pub fn new(url: &str, allow_restricted: bool) -> Result<Self> {
        let safe_url = validate_url(url, allow_restricted)?;
        let handle = global_handle().map_err(Error::Io)?;
        let client = Client::builder()
            .redirect(Policy::none())
            .build()
            .map_err(|e| Error::Io(IoError::other(e)))?;

        let len = tokio::task::block_in_place(|| {
            handle.block_on(async {
                let resp = send_with_redirects(
                    &client,
                    reqwest::Method::HEAD,
                    &safe_url,
                    HeaderMap::new(),
                )
                .await?;
                resp.headers()
                    .get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .ok_or_else(|| {
                        Error::Io(IoError::new(
                            ErrorKind::InvalidData,
                            "Missing Content-Length header",
                        ))
                    })
            })
        })?;

        Ok(Self {
            url: safe_url,
            client,
            len,
            handle,
        })
    }
}

impl StorageBackend for HttpBackend {
    fn read_exact(&self, offset: u64, len: usize) -> Result<Bytes> {
        if len == 0 {
            return Ok(Bytes::new());
        }
        let end = offset + len as u64 - 1;
        let mut headers = HeaderMap::new();
        _ = headers.insert(
            reqwest::header::RANGE,
            format!("bytes={offset}-{end}")
                .parse()
                .map_err(|e: reqwest::header::InvalidHeaderValue| Error::Io(IoError::other(e)))?,
        );

        tokio::task::block_in_place(|| {
            self.handle.block_on(async {
                let resp =
                    send_with_redirects(&self.client, reqwest::Method::GET, &self.url, headers)
                        .await?;
                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| Error::Io(IoError::other(e)))?;
                if bytes.len() != len {
                    return Err(Error::Io(IoError::new(
                        ErrorKind::UnexpectedEof,
                        format!("Expected {} bytes, got {}", len, bytes.len()),
                    )));
                }
                Ok(bytes)
            })
        })
    }

    fn len(&self) -> u64 {
        self.len
    }
}
