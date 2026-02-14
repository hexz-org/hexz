//! System utility commands for diagnostics and services.
//!
//! This module provides system-level utilities for troubleshooting, performance
//! testing, network serving, and cryptographic operations.
//!
//! # Available Commands
//!
//! ## Diagnostics (feature = "diagnostics")
//!
//! - [`doctor`]: Run comprehensive system diagnostics
//!   - Check for required dependencies (QEMU, KVM, FUSE)
//!   - Verify storage backend connectivity
//!   - Test compression and encryption functionality
//!
//! - [`mod@bench`]: Benchmark archive performance
//!   - Measure read throughput at various block sizes
//!   - Test cache effectiveness
//!   - Profile compression/decompression speed
//!
//! ## Network Serving (feature = "server")
//!
//! - [`serve`]: Serve archives over network protocols
//!   - NBD (Network Block Device) protocol
//!   - S3-compatible API
//!   - HTTP range requests
//!
//! ## Cryptographic Operations (feature = "signing")
//!
//! - [`keygen`]: Generate Ed25519 signing key pairs
//! - [`sign`]: Sign archives with private keys
//! - [`verify`]: Verify archive signatures with public keys
//!
//! # Usage Examples
//!
//! ```bash
//! # Check system health
//! hexz sys doctor
//!
//! # Benchmark an archive
//! hexz sys bench snapshot.st --threads 8 --duration 60
//!
//! # Serve via NBD
//! hexz sys serve snapshot.st --nbd --port 10809
//!
//! # Generate and use signing keys
//! hexz sys keygen --output-dir ~/.hexz/keys
//! hexz sys sign --key ~/.hexz/keys/private.key snapshot.st
//! hexz sys verify --key ~/.hexz/keys/public.key snapshot.st
//! ```
//!
//! # Feature Flags
//!
//! These commands require specific feature flags to be enabled at compile time:
//! - `diagnostics`: doctor, bench
//! - `server`: serve
//! - `signing`: keygen, sign, verify

#[cfg(feature = "diagnostics")]
pub mod doctor;

#[cfg(feature = "diagnostics")]
pub mod bench;

#[cfg(feature = "server")]
pub mod serve;

#[cfg(feature = "signing")]
pub mod keygen;

#[cfg(feature = "signing")]
pub mod sign;

#[cfg(feature = "signing")]
pub mod verify;
