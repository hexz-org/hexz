//! Reconstruct a safetensors file from a Hexz archive.

use anyhow::Result;
use hexz_ops::safetensors::extract_safetensors;
use std::path::PathBuf;

/// Execute the `hexz extract` command.
pub fn run(input: PathBuf, output: Option<PathBuf>, tensor: Option<String>) -> Result<()> {
    let output = match output {
        Some(p) => p,
        None => input.with_extension("safetensors"),
    };

    extract_safetensors(&input, &output, tensor.as_deref())?;
    println!("Extracted: {:?}", output);
    Ok(())
}
