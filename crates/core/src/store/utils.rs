//! URL validation utilities for storage backends.
//!
//! This module provides security-focused URL validation functions used by
//! HTTP and S3 storage backends to prevent SSRF (Server-Side Request Forgery)
//! attacks. It detects and blocks access to internal networks, loopback addresses,
//! and cloud metadata endpoints.

use std::io::{Error, ErrorKind};
use std::net::{IpAddr, ToSocketAddrs};
use strata_common::{Result, StrataError};
use url::{Host, Url};

/// Checks if an IP address belongs to a restricted range.
///
/// Restricted ranges include:
/// - **IPv4**:
///   - Loopback: 127.0.0.0/8
///   - Private: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
///   - Link-local/metadata: 169.254.0.0/16 (AWS metadata: 169.254.169.254)
/// - **IPv6**:
///   - Loopback: ::1
///   - Unique local: fc00::/7
///   - Link-local: fe80::/10
///
/// # Parameters
///
/// - `ip`: The IP address to check
///
/// # Returns
///
/// `true` if the IP is in a restricted range, `false` otherwise.
///
/// # Examples
///
/// ```
/// use std::net::IpAddr;
/// use strata_core::store::utils::is_restricted_ip;
///
/// // Loopback is restricted
/// assert!(is_restricted_ip("127.0.0.1".parse::<IpAddr>().unwrap()));
///
/// // Private network is restricted
/// assert!(is_restricted_ip("192.168.1.1".parse::<IpAddr>().unwrap()));
///
/// // Public IP is not restricted
/// assert!(!is_restricted_ip("8.8.8.8".parse::<IpAddr>().unwrap()));
/// ```
pub fn is_restricted_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            let octets = ipv4.octets();
            // 127.0.0.0/8 (Loopback)
            if octets[0] == 127 {
                return true;
            }
            // 10.0.0.0/8 (Private)
            if octets[0] == 10 {
                return true;
            }
            // 172.16.0.0/12 (Private)
            if octets[0] == 172 && (octets[1] >= 16 && octets[1] <= 31) {
                return true;
            }
            // 192.168.0.0/16 (Private)
            if octets[0] == 192 && octets[1] == 168 {
                return true;
            }
            // 169.254.0.0/16 (Link-Local / Cloud Metadata)
            if octets[0] == 169 && octets[1] == 254 {
                return true;
            }
            false
        }
        IpAddr::V6(ipv6) => {
            if ipv6.is_loopback() {
                return true;
            }
            let segments = ipv6.segments();
            // fc00::/7 (Unique Local)
            if (segments[0] & 0xfe00) == 0xfc00 {
                return true;
            }
            // fe80::/10 (Link-Local)
            if (segments[0] & 0xffc0) == 0xfe80 {
                return true;
            }
            false
        }
    }
}

/// Validates and sanitizes a URL for safe remote access.
///
/// This function performs comprehensive validation to prevent SSRF attacks:
/// 1. Parses the URL and checks scheme (only HTTP/HTTPS allowed)
/// 2. Extracts the hostname
/// 3. Resolves domain names to IP addresses via DNS
/// 4. Checks all resolved IPs against restricted ranges
/// 5. Returns the sanitized URL if validation passes
///
/// # Parameters
///
/// - `url_str`: The URL string to validate
/// - `allow_restricted`: If `true`, skips IP restriction checks (dangerous!)
///
/// # Returns
///
/// - `Ok(String)`: The validated and normalized URL
/// - `Err(StrataError::Io)`: If URL is malformed, uses invalid scheme, or points to restricted IP
///
/// # Security
///
/// Always use `allow_restricted: false` in production unless you have a specific
/// trusted environment. Allowing restricted IPs can enable:
/// - Access to cloud metadata endpoints (AWS: 169.254.169.254)
/// - Internal service discovery and enumeration
/// - Port scanning of private networks
///
/// # Examples
///
/// ```
/// use strata_core::store::utils::validate_url;
///
/// // Valid public URL
/// assert!(validate_url("https://example.com/file.st", false).is_ok());
///
/// // Invalid scheme
/// assert!(validate_url("ftp://example.com/file.st", false).is_err());
///
/// // Restricted IP (blocked by default)
/// assert!(validate_url("http://127.0.0.1/file.st", false).is_err());
///
/// // Restricted IP (allowed with flag)
/// assert!(validate_url("http://127.0.0.1/file.st", true).is_ok());
/// ```
pub fn validate_url(url_str: &str, allow_restricted: bool) -> Result<String> {
    let url = Url::parse(url_str).map_err(|e| {
        StrataError::Io(Error::new(
            ErrorKind::InvalidInput,
            format!("Invalid URL: {}", e),
        ))
    })?;

    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(StrataError::Io(Error::new(
            ErrorKind::InvalidInput,
            "Only HTTP and HTTPS schemes are allowed",
        )));
    }

    // If restricted IPs are allowed, we can skip the IP checks
    if allow_restricted {
        return Ok(url.to_string());
    }

    let host = url
        .host()
        .ok_or_else(|| StrataError::Io(Error::new(ErrorKind::InvalidInput, "URL missing host")))?;

    match host {
        Host::Ipv4(ip) => {
            if is_restricted_ip(IpAddr::V4(ip)) {
                return Err(StrataError::Io(Error::new(
                    ErrorKind::PermissionDenied,
                    format!("Access to internal/private IP denied: {}", ip),
                )));
            }
        }
        Host::Ipv6(ip) => {
            if is_restricted_ip(IpAddr::V6(ip)) {
                return Err(StrataError::Io(Error::new(
                    ErrorKind::PermissionDenied,
                    format!("Access to internal/private IP denied: {}", ip),
                )));
            }
        }
        Host::Domain(domain) => {
            // Defensive: Strip brackets if they somehow ended up in the domain string
            let clean_domain = if domain.starts_with('[') && domain.ends_with(']') {
                &domain[1..domain.len() - 1]
            } else {
                domain
            };

            // Try parsing as IP first to avoid DNS lookup for literals
            if let Ok(ip) = clean_domain.parse::<IpAddr>() {
                if is_restricted_ip(ip) {
                    return Err(StrataError::Io(Error::new(
                        ErrorKind::PermissionDenied,
                        format!("Access to internal/private IP denied: {}", ip),
                    )));
                }
                return Ok(url.to_string());
            }

            let port = url.port_or_known_default().unwrap_or(80);

            let addrs = (clean_domain, port).to_socket_addrs().map_err(|e| {
                StrataError::Io(Error::other(format!(
                    "DNS resolution failed for domain '{}': {}",
                    clean_domain, e
                )))
            })?;

            for addr in addrs {
                if is_restricted_ip(addr.ip()) {
                    return Err(StrataError::Io(Error::new(
                        ErrorKind::PermissionDenied,
                        format!("Access to internal/private IP denied: {}", addr.ip()),
                    )));
                }
            }
        }
    }

    Ok(url.to_string())
}
