//! Extract data from a Hexz archive.

use anyhow::{Context, Result};
use hexz_ops::pack::extract_archive;
use hexz_core::format::header::Header;
use hexz_core::format::magic::HEADER_SIZE;
use hexz_store::local::MmapBackend;
use hexz_store::StorageBackend;
use std::path::PathBuf;
use colored::*;

/// Execute the `hexz extract` command.
pub fn run(input: PathBuf, output: Option<PathBuf>) -> Result<()> {
    // 1. Resolve output path
    let output = match output {
        Some(p) => p,
        None => {
            // Default: if it has a manifest, use dir name, otherwise .bin
            let mut out = input.clone();
            out.set_extension("");
            out
        }
    };

    // 2. Check for encryption
    let password = {
        let backend = MmapBackend::new(&input)?;
        let header_bytes = backend.read_exact(0, HEADER_SIZE)?;
        let header: Header = bincode::deserialize(&header_bytes)?;

        if header.encryption.is_some() {
            Some(match std::env::var("HEXZ_PASSWORD") {
                Ok(p) => p,
                Err(_) => rpassword::prompt_password("Enter decryption password: ")?,
            })
        } else {
            None
        }
    };

    println!("{} Extracting {}", "╭".dimmed(), input.display().to_string().cyan());
    println!("{} Output     {}", "╰".dimmed(), output.display().to_string().bright_black());

    extract_archive(&input, &output, password).context("Failed to extract archive")?;

    println!("\n  {} Extraction complete.", "✓".green());

    Ok(())
}
