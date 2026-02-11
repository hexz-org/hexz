# strata-common

Common utilities, configuration, and shared types for the Strata ecosystem.

## Overview

`strata-common` centralizes configuration loading, error types, logging setup, and format constants that are shared across all Strata crates (core, CLI, FUSE, server, loader). This ensures a single source of truth for defaults, wire formats, and runtime parameters.

This is an internal crate used by other Strata components—it's not typically used directly by end users.

## Architecture

```
strata-common/
├── src/
│   ├── lib.rs          # Public exports
│   ├── config.rs       # Configuration loading and management
│   ├── constants.rs    # Format constants (magic bytes, sizes, versions)
│   ├── error.rs        # Shared error types (StrataError, Result)
│   ├── logging.rs      # Logging initialization (tracing)
│   ├── crypto.rs       # Cryptographic utilities
│   └── sign.rs         # Ed25519 signing (optional feature)
```

## Key Components

### Configuration (`config.rs`)

Runtime parameter management for Strata tools:

```rust
use strata_common::Config;

// Load configuration (from env, files, defaults)
let config = Config::load()?;

// Access settings
println!("Cache size: {}", config.cache_size);
println!("Log level: {}", config.log_level);
```

### Constants (`constants.rs`)

Format constants and wire format definitions:

```rust
use strata_common::constants::*;

// Magic bytes for format identification
assert_eq!(MAGIC, b"STRATA\x00\x00");

// Header and block sizes
println!("Header size: {} bytes", HEADER_SIZE);
println!("Default block size: {} bytes", DEFAULT_BLOCK_SIZE);

// Format version
println!("Format version: {}", FORMAT_VERSION);
```

These constants are used across all crates to ensure consistent on-disk layout.

### Error Types (`error.rs`)

Shared error types using `thiserror`:

```rust
use strata_common::{Result, StrataError};

fn read_header() -> Result<Vec<u8>> {
    // Returns Result<T> = std::result::Result<T, StrataError>
    Err(StrataError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "File not found"
    )))
}
```

Common error variants:
- `Io(io::Error)` - I/O errors
- `Format(String)` - Invalid format or corruption
- `Compression(String)` - Compression/decompression failures
- `Encryption(String)` - Encryption/decryption failures
- `InvalidParameter(String)` - Invalid configuration

### Logging (`logging.rs`)

Unified logging setup using `tracing`:

```rust
use strata_common::logging;

fn main() {
    // Initialize logging with default settings
    logging::init();

    // Now all crates can use tracing macros
    tracing::info!("Application started");
    tracing::debug!("Debug information");
}
```

### Cryptographic Utilities (`crypto.rs`)

Password-based key derivation parameters:

```rust
use strata_common::crypto;

// Key derivation for encryption
let salt = crypto::generate_salt();
let key = crypto::derive_key(password, &salt)?;
```

### Signing (`sign.rs`) - Optional Feature

Ed25519 signing for snapshot integrity (requires `signing` feature):

```rust
use strata_common::sign::{Keypair, sign, verify};

// Generate keypair
let keypair = Keypair::generate();

// Sign data
let signature = sign(&keypair, b"snapshot data");

// Verify signature
verify(&keypair.public, b"snapshot data", &signature)?;
```

## Usage in Other Crates

### Core Library

```rust
use strata_common::{Result, StrataError};
use strata_common::constants::*;

fn validate_header(magic: &[u8]) -> Result<()> {
    if magic != MAGIC {
        return Err(StrataError::Format(
            "Invalid magic bytes".to_string()
        ));
    }
    Ok(())
}
```

### CLI Tool

```rust
use strata_common::{Config, logging};

fn main() -> anyhow::Result<()> {
    // Initialize logging
    logging::init();

    // Load configuration
    let config = Config::load()?;

    // Use shared error types
    let result = operation();
    match result {
        Ok(data) => println!("Success"),
        Err(e) => eprintln!("Error: {}", e),
    }

    Ok(())
}
```

## Features

- `default`: No features enabled by default
- `signing`: Enable Ed25519 signing support

Build with signing:

```bash
cargo build -p strata-common --features signing
```

## Development

From the repository root:

```bash
# Build the common crate
cargo build -p strata-common

# Run tests
cargo test -p strata-common

# Build with all features
cargo build -p strata-common --all-features
```

## Constants Reference

Key constants defined in `constants.rs`:

| Constant | Value | Description |
|----------|-------|-------------|
| `MAGIC` | `b"STRATA\x00\x00"` | File format magic bytes |
| `FORMAT_VERSION` | `1` | Current format version |
| `HEADER_SIZE` | `512` | Header size in bytes |
| `DEFAULT_BLOCK_SIZE` | `65536` | Default compression block size (64KB) |
| `MAX_BLOCK_SIZE` | `16777216` | Maximum block size (16MB) |
| `DEFAULT_CACHE_SIZE` | `1024` | Default LRU cache size (blocks) |

These values affect on-disk format and must be coordinated across all crates.

## See Also

- **[strata-core](../core/)** - Core engine (uses common types)
- **[strata-cli](../cli/)** - CLI tool (uses config and logging)
- **[Project README](../../README.md)** - Main project overview
