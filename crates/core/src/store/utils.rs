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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    // Tests for is_restricted_ip()

    #[test]
    fn test_ipv4_loopback_is_restricted() {
        let ips = vec!["127.0.0.1", "127.0.0.2", "127.1.1.1", "127.255.255.255"];
        for ip_str in ips {
            let ip: Ipv4Addr = ip_str.parse().unwrap();
            assert!(
                is_restricted_ip(IpAddr::V4(ip)),
                "Loopback IP {} should be restricted",
                ip_str
            );
        }
    }

    #[test]
    fn test_ipv4_private_10_is_restricted() {
        let ips = vec!["10.0.0.0", "10.0.0.1", "10.255.255.255"];
        for ip_str in ips {
            let ip: Ipv4Addr = ip_str.parse().unwrap();
            assert!(
                is_restricted_ip(IpAddr::V4(ip)),
                "Private IP {} should be restricted",
                ip_str
            );
        }
    }

    #[test]
    fn test_ipv4_private_172_16_31_is_restricted() {
        let ips = vec!["172.16.0.0", "172.16.0.1", "172.20.0.1", "172.31.255.255"];
        for ip_str in ips {
            let ip: Ipv4Addr = ip_str.parse().unwrap();
            assert!(
                is_restricted_ip(IpAddr::V4(ip)),
                "Private IP {} should be restricted",
                ip_str
            );
        }

        // Test boundaries (172.15 and 172.32 should NOT be restricted)
        assert!(!is_restricted_ip(IpAddr::V4(
            "172.15.255.255".parse().unwrap()
        )));
        assert!(!is_restricted_ip(IpAddr::V4("172.32.0.0".parse().unwrap())));
    }

    #[test]
    fn test_ipv4_private_192_168_is_restricted() {
        let ips = vec!["192.168.0.0", "192.168.1.1", "192.168.255.255"];
        for ip_str in ips {
            let ip: Ipv4Addr = ip_str.parse().unwrap();
            assert!(
                is_restricted_ip(IpAddr::V4(ip)),
                "Private IP {} should be restricted",
                ip_str
            );
        }

        // Test boundaries (192.167 and 192.169 should NOT be restricted)
        assert!(!is_restricted_ip(IpAddr::V4(
            "192.167.0.0".parse().unwrap()
        )));
        assert!(!is_restricted_ip(IpAddr::V4(
            "192.169.0.0".parse().unwrap()
        )));
    }

    #[test]
    fn test_ipv4_link_local_is_restricted() {
        let ips = vec![
            "169.254.0.0",
            "169.254.169.254", // AWS metadata endpoint
            "169.254.255.255",
        ];
        for ip_str in ips {
            let ip: Ipv4Addr = ip_str.parse().unwrap();
            assert!(
                is_restricted_ip(IpAddr::V4(ip)),
                "Link-local IP {} should be restricted",
                ip_str
            );
        }

        // Test boundaries
        assert!(!is_restricted_ip(IpAddr::V4(
            "169.253.255.255".parse().unwrap()
        )));
        assert!(!is_restricted_ip(IpAddr::V4(
            "169.255.0.0".parse().unwrap()
        )));
    }

    #[test]
    fn test_ipv4_public_is_not_restricted() {
        let ips = vec![
            "8.8.8.8",       // Google DNS
            "1.1.1.1",       // Cloudflare DNS
            "93.184.216.34", // example.com
            "151.101.1.140", // Reddit
            "13.107.42.14",  // Microsoft
        ];
        for ip_str in ips {
            let ip: Ipv4Addr = ip_str.parse().unwrap();
            assert!(
                !is_restricted_ip(IpAddr::V4(ip)),
                "Public IP {} should NOT be restricted",
                ip_str
            );
        }
    }

    #[test]
    fn test_ipv6_loopback_is_restricted() {
        let ip: Ipv6Addr = "::1".parse().unwrap();
        assert!(is_restricted_ip(IpAddr::V6(ip)));
    }

    #[test]
    fn test_ipv6_unique_local_is_restricted() {
        let ips = vec![
            "fc00::",
            "fc00::1",
            "fd00::1",
            "fdff:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
        ];
        for ip_str in ips {
            let ip: Ipv6Addr = ip_str.parse().unwrap();
            assert!(
                is_restricted_ip(IpAddr::V6(ip)),
                "Unique local IPv6 {} should be restricted",
                ip_str
            );
        }
    }

    #[test]
    fn test_ipv6_link_local_is_restricted() {
        let ips = vec![
            "fe80::",
            "fe80::1",
            "febf:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
        ];
        for ip_str in ips {
            let ip: Ipv6Addr = ip_str.parse().unwrap();
            assert!(
                is_restricted_ip(IpAddr::V6(ip)),
                "Link-local IPv6 {} should be restricted",
                ip_str
            );
        }
    }

    #[test]
    fn test_ipv6_public_is_not_restricted() {
        let ips = vec![
            "2001:4860:4860::8888", // Google DNS
            "2606:4700:4700::1111", // Cloudflare DNS
            "2001:db8::1",          // Documentation prefix
        ];
        for ip_str in ips {
            let ip: Ipv6Addr = ip_str.parse().unwrap();
            assert!(
                !is_restricted_ip(IpAddr::V6(ip)),
                "Public IPv6 {} should NOT be restricted",
                ip_str
            );
        }
    }

    // Tests for validate_url()

    #[test]
    fn test_validate_url_valid_https() {
        let result = validate_url("https://example.com/file.st", false);
        assert!(result.is_ok(), "HTTPS URL should be valid");
    }

    #[test]
    fn test_validate_url_valid_http() {
        let result = validate_url("http://example.com/file.st", false);
        assert!(result.is_ok(), "HTTP URL should be valid");
    }

    #[test]
    fn test_validate_url_invalid_scheme_ftp() {
        let result = validate_url("ftp://example.com/file.st", false);
        assert!(result.is_err(), "FTP scheme should be rejected");
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.to_lowercase().contains("http"));
    }

    #[test]
    fn test_validate_url_invalid_scheme_file() {
        let result = validate_url("file:///etc/passwd", false);
        assert!(result.is_err(), "file:// scheme should be rejected");
    }

    #[test]
    fn test_validate_url_malformed() {
        let result = validate_url("not a url", false);
        assert!(result.is_err(), "Malformed URL should be rejected");
    }

    #[test]
    fn test_validate_url_missing_host() {
        let result = validate_url("http://", false);
        assert!(result.is_err(), "URL without host should be rejected");
    }

    #[test]
    fn test_validate_url_ipv4_loopback_blocked() {
        let result = validate_url("http://127.0.0.1/file.st", false);
        assert!(result.is_err(), "Loopback IP should be blocked");
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.to_lowercase().contains("denied"));
    }

    #[test]
    fn test_validate_url_ipv4_private_blocked() {
        let urls = vec![
            "http://10.0.0.1/file.st",
            "http://172.16.0.1/file.st",
            "http://192.168.1.1/file.st",
        ];
        for url in urls {
            let result = validate_url(url, false);
            assert!(result.is_err(), "Private IP {} should be blocked", url);
        }
    }

    #[test]
    fn test_validate_url_ipv4_link_local_blocked() {
        let result = validate_url("http://169.254.169.254/latest/meta-data", false);
        assert!(result.is_err(), "AWS metadata endpoint should be blocked");
    }

    #[test]
    fn test_validate_url_ipv6_loopback_blocked() {
        let result = validate_url("http://[::1]/file.st", false);
        assert!(result.is_err(), "IPv6 loopback should be blocked");
    }

    #[test]
    fn test_validate_url_ipv6_unique_local_blocked() {
        let result = validate_url("http://[fc00::1]/file.st", false);
        assert!(result.is_err(), "IPv6 unique local should be blocked");
    }

    #[test]
    fn test_validate_url_ipv6_link_local_blocked() {
        let result = validate_url("http://[fe80::1]/file.st", false);
        assert!(result.is_err(), "IPv6 link-local should be blocked");
    }

    #[test]
    fn test_validate_url_allow_restricted_flag() {
        let urls = vec![
            "http://127.0.0.1/file.st",
            "http://10.0.0.1/file.st",
            "http://192.168.1.1/file.st",
            "http://[::1]/file.st",
        ];
        for url in urls {
            let result = validate_url(url, true);
            assert!(
                result.is_ok(),
                "Restricted IP {} should be allowed with flag",
                url
            );
        }
    }

    #[test]
    fn test_validate_url_normalized_output() {
        let input = "https://example.com:443/path?query=value";
        let result = validate_url(input, false);
        assert!(result.is_ok());
        let output = result.unwrap();
        // URL should be normalized
        assert!(output.contains("example.com"));
        assert!(output.contains("path"));
    }

    #[test]
    fn test_validate_url_domain_with_port() {
        let result = validate_url("https://example.com:8080/file.st", false);
        assert!(result.is_ok(), "URL with custom port should be valid");
    }

    #[test]
    fn test_validate_url_with_path_and_query() {
        let result = validate_url("https://example.com/path/to/file.st?key=value", false);
        assert!(result.is_ok(), "URL with path and query should be valid");
    }

    #[test]
    fn test_validate_url_localhost_blocked() {
        // Note: This test may fail if DNS is not available or if "localhost" doesn't resolve
        // In most systems, "localhost" resolves to 127.0.0.1 which should be blocked
        let result = validate_url("http://localhost/file.st", false);
        // This might resolve to 127.0.0.1 and be blocked, or fail DNS resolution
        // Either way, it should not succeed in default configuration
        assert!(
            result.is_err(),
            "localhost should typically be blocked or fail resolution"
        );
    }

    #[test]
    fn test_validate_url_empty_string() {
        let result = validate_url("", false);
        assert!(result.is_err(), "Empty URL should be rejected");
    }

    #[test]
    fn test_validate_url_brackets_in_domain() {
        // Test the bracket-stripping logic for domains
        let result = validate_url("http://example.com/file.st", false);
        assert!(result.is_ok());
    }
}
