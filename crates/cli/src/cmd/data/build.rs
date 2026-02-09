//! Build archive from source directory.
//!
//! This command provides a higher-level interface for creating Strata snapshots,
//! supporting profiles (generic, eda, embedded, ml) and unified source handling.

use anyhow::Result;
use std::path::PathBuf;
use strata_common::config::BuildProfile;

/// Execute the build command.
pub fn run(
    source: PathBuf,
    memory: Option<PathBuf>,
    output: PathBuf,
    profile: Option<String>,
    encrypt: bool,
    cdc: bool,
) -> Result<()> {
    // 1. Resolve profile
    let build_profile = match profile.as_deref() {
        Some("eda") => BuildProfile::Eda,
        Some("embedded") => BuildProfile::Embedded,
        Some("ml") => BuildProfile::Ml,
        Some("generic") | None => BuildProfile::Generic,
        Some(other) => {
            eprintln!(
                "Warning: Unknown profile '{}', falling back to generic",
                other
            );
            BuildProfile::Generic
        }
    };

    println!("Building snapshot with profile: {:?}", build_profile);

    // 2. Map profile to parameters
    let compression = build_profile.compression_algo().to_string();
    let block_size = build_profile.block_size();
    let train_dict = build_profile.recommended_dict_training();

    // 3. Delegate to pack
    // Note: We currently map `source` directly to `disk`.
    // Future work: Detect if `source` is a directory and pack it (e.g. tar/squashfs)
    // or use `virt-make-fs`.
    super::pack::run(
        Some(source),
        memory,
        output,
        compression,
        encrypt,
        train_dict,
        block_size,
        cdc,
        16384,  // min_chunk default
        65536,  // avg_chunk default
        131072, // max_chunk default
        false,  // silent
    )
}
