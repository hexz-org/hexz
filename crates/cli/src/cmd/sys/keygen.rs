//! Ed25519 key pair generation for archive signing.
//!
//! This command generates cryptographic signing keys used to sign and verify
//! Hexz archives, ensuring authenticity and integrity.
//!
//! # Key Generation
//!
//! The `keygen` command creates an Ed25519 key pair:
//! - **Private key** (`private.key`): Used to sign archives
//! - **Public key** (`public.key`): Used to verify signatures
//!
//! # Security Considerations
//!
//! - **Private Key Protection**: Store private keys securely with restricted permissions (chmod 600)
//! - **Key Distribution**: Share public keys safely; they can be freely distributed
//! - **Backup**: Maintain secure backups of private keys to prevent loss
//!
//! # Usage
//!
//! ```bash
//! # Generate keys in current directory
//! hexz sys keygen
//!
//! # Generate keys in specific directory
//! hexz sys keygen --output-dir ~/.hexz/keys
//!
//! # Secure the private key
//! chmod 600 ~/.hexz/keys/private.key
//! ```
//!
//! # Integration with Archive Signing
//!
//! After generating keys, use them to sign archives:
//!
//! ```bash
//! # Sign an archive
//! hexz sys sign --key private.key archive.st
//!
//! # Verify the signature
//! hexz sys verify --key public.key archive.st
//! ```
//!
//! # Implementation
//!
//! Uses Ed25519 signatures via the `ed25519-dalek` crate for:
//! - Fast signature generation and verification
//! - 64-byte signatures
//! - Strong cryptographic security (128-bit security level)

use anyhow::Result;
use hexz_common::sign;
use std::path::PathBuf;

/// Generate an Ed25519 signing key pair.
///
/// This function creates a new Ed25519 private/public key pair and saves them
/// to the specified output directory (or current directory if not specified).
///
/// # Arguments
///
/// * `output_dir` - Optional directory to store keys. Defaults to current directory.
///
/// # Generated Files
///
/// - `private.key`: Ed25519 private key (32 bytes, keep secure!)
/// - `public.key`: Ed25519 public key (32 bytes, can be shared)
///
/// # Returns
///
/// Returns `Ok(())` on success, or an error if key generation or file writing fails.
///
/// # Security Warning
///
/// The private key file MUST be protected with appropriate filesystem permissions:
///
/// ```bash
/// chmod 600 private.key
/// ```
///
/// # Example
///
/// ```no_run
/// # use std::path::PathBuf;
/// # use hexz_cli::cmd::sys::keygen;
/// // Generate keys in ~/.hexz/keys
/// keygen::run(Some(PathBuf::from("/home/user/.hexz/keys")))?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn run(output_dir: Option<PathBuf>) -> Result<()> {
    let dir = match output_dir {
        Some(d) => d,
        None => std::env::current_dir()?,
    };
    let priv_path = dir.join("private.key");
    let pub_path = dir.join("public.key");

    use colored::*;
    println!("{} Generating Ed25519 keypair", "╭".dimmed());
    sign::generate_keypair(&priv_path, &pub_path)?;

    println!("{} Private   {}", "│".dimmed(), priv_path.display().to_string().cyan());
    println!("{} Public    {}", "╰".dimmed(), pub_path.display().to_string().cyan());

    println!("\n  {} Keys generated successfully.", "✓".green());
    println!("  {} Keep the private key safe!", "→".yellow());

    Ok(())
}
