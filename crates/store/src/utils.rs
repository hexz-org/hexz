//! URL validation utilities — SSRF prevention for HTTP and S3 backends.

use hexz_common::{Error, Result};
use std::io::{Error as IoError, ErrorKind};
use std::net::{IpAddr, ToSocketAddrs};
use url::{Host, Url};

/// Returns `true` if `ip` is a loopback, private, or link-local address.
pub fn is_restricted_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 127
                || o[0] == 10
                || (o[0] == 172 && (16..=31).contains(&o[1]))
                || (o[0] == 192 && o[1] == 168)
                || (o[0] == 169 && o[1] == 254)
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() {
                return true;
            }
            let s = v6.segments();
            (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80
        }
    }
}

/// Validates and sanitises a URL, blocking internal/private addresses unless
/// `allow_restricted` is set.
pub fn validate_url(url_str: &str, allow_restricted: bool) -> Result<String> {
    let url = Url::parse(url_str).map_err(|e| {
        Error::Io(IoError::new(
            ErrorKind::InvalidInput,
            format!("Invalid URL: {e}"),
        ))
    })?;

    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(Error::Io(IoError::new(
            ErrorKind::InvalidInput,
            "Only HTTP and HTTPS schemes are allowed",
        )));
    }

    if allow_restricted {
        return Ok(url.to_string());
    }

    let host = url
        .host()
        .ok_or_else(|| Error::Io(IoError::new(ErrorKind::InvalidInput, "URL missing host")))?;

    match host {
        Host::Ipv4(ip) => {
            if is_restricted_ip(IpAddr::V4(ip)) {
                return Err(Error::Io(IoError::new(
                    ErrorKind::PermissionDenied,
                    format!("Access to internal/private IP denied: {ip}"),
                )));
            }
        }
        Host::Ipv6(ip) => {
            if is_restricted_ip(IpAddr::V6(ip)) {
                return Err(Error::Io(IoError::new(
                    ErrorKind::PermissionDenied,
                    format!("Access to internal/private IP denied: {ip}"),
                )));
            }
        }
        Host::Domain(domain) => {
            let clean = if domain.starts_with('[') && domain.ends_with(']') {
                &domain[1..domain.len() - 1]
            } else {
                domain
            };

            if let Ok(ip) = clean.parse::<IpAddr>() {
                if is_restricted_ip(ip) {
                    return Err(Error::Io(IoError::new(
                        ErrorKind::PermissionDenied,
                        format!("Access to internal/private IP denied: {ip}"),
                    )));
                }
                return Ok(url.to_string());
            }

            let port = url.port_or_known_default().unwrap_or(80);
            let addrs = (clean, port).to_socket_addrs().map_err(|e| {
                Error::Io(IoError::other(format!(
                    "DNS resolution failed for '{clean}': {e}"
                )))
            })?;

            for addr in addrs {
                if is_restricted_ip(addr.ip()) {
                    return Err(Error::Io(IoError::new(
                        ErrorKind::PermissionDenied,
                        format!("Access to internal/private IP denied: {}", addr.ip()),
                    )));
                }
            }
        }
    }

    Ok(url.to_string())
}
