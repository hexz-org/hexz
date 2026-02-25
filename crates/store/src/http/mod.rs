//! HTTP/HTTPS storage backend using `reqwest` with manual redirect following.
//!
//! Range requests are re-sent verbatim on each redirect hop so that CDN
//! cross-origin redirects (e.g. GitHub Releases → Azure Blob) work correctly.

pub mod async_client;

pub use async_client::HttpBackend;
