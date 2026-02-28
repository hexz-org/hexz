//! Store a safetensors file as a Hexz archive.

use anyhow::Result;
use hexz_ops::safetensors::{SafetensorsStoreConfig, store_safetensors};
use std::path::PathBuf;

/// Execute the `hexz store` command.
pub fn run(
    input: PathBuf,
    output: PathBuf,
    base: Option<PathBuf>,
    compression: String,
    block_size: u32,
    silent: bool,
) -> Result<()> {
    let config = SafetensorsStoreConfig {
        input,
        output: output.clone(),
        base,
        compression,
        block_size,
        show_progress: !silent,
    };

    let summary = store_safetensors(config)?;

    if !silent {
        println!(
            "Stored {} tensor(s): {:.2} MB in → {:.2} MB out ({:.1}s)",
            summary.tensors,
            summary.total_bytes as f64 / 1_048_576.0,
            summary.stored_bytes as f64 / 1_048_576.0,
            summary.elapsed_secs,
        );
        println!("Archive: {:?}", output);
    }

    Ok(())
}
