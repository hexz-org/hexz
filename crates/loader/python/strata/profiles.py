"""Pre-configured build profiles for common use cases.

Build profiles provide optimized configurations for different scenarios:

- ml: Machine learning datasets (fast writes, sequential reads)
- eda: Exploratory data analysis (balanced)
- embedded: Resource-constrained environments (max compression)
- generic: General purpose default
- archival: Long-term storage (max compression and dedup)
"""

from typing import Dict, Any, Literal, Optional
from pathlib import Path
import os

from .typing import BuildProfile, PathLike
from .writer import Writer
from .utils import Metadata, inspect
from .exceptions import ValidationError

# Build profiles map to specific Writer configurations
PROFILES: Dict[BuildProfile, Dict[str, Any]] = {
    # Machine Learning: Fast writes, good compression, optimized for sequential access
    "ml": {
        "mode": "fast",
        "block_size": 128 * 1024,  # Large blocks for sequential reads
        "dedup": True,  # Enable deduplication
        "compression": "lz4",  # Fast compression/decompression
    },
    # Exploratory Data Analysis: Balanced, good for notebooks and experiments
    "eda": {
        "mode": "balanced",
        "block_size": 64 * 1024,
        "dedup": True,
        "compression": "lz4",
    },
    # Embedded: Maximum compression for resource-constrained environments
    "embedded": {
        "mode": "tight",
        "block_size": 32 * 1024,  # Smaller blocks
        "dedup": True,  # Full dedup
        "compression": "zstd",
    },
    # Generic: Balanced defaults for general use
    "generic": {
        "mode": "balanced",
        "block_size": 64 * 1024,
        "dedup": True,
        "compression": "lz4",
    },
    # Archival: Maximum compression and dedup for long-term storage
    "archival": {
        "mode": "tight",
        "block_size": 256 * 1024,  # Very large blocks
        "dedup": True,
        "compression": "zstd",
    },
}


def build(
    source: PathLike,
    output: PathLike,
    *,
    profile: BuildProfile = "generic",
    **overrides: Any,
) -> Metadata:
    """Build a snapshot using a preset profile.

    This is a convenience function that combines Writer configuration
    and common build patterns.

    Args:
        source: Source file, directory, or data
        output: Output .st file path
        profile: Build profile to use
        **overrides: Override any profile settings

    Returns:
        Metadata object with snapshot information

    Example:
        >>> # ML dataset with defaults
        >>> meta = strata.build("imagenet/", "imagenet.st", profile="ml")
        >>> print(f"Compressed to {meta.size_compressed / 1e9:.1f} GB")
        ...
        >>> # Archival with encryption
        >>> meta = strata.build(
        ...     "backup/",
        ...     "backup.st",
        ...     profile="archival",
        ...     encrypt=True,
        ...     password="secret",
        ... )
    """
    # Get profile configuration
    config = PROFILES[profile].copy()

    # Apply overrides
    config.update(overrides)

    # Convert source to Path for easier handling
    source_path = Path(source)

    # Build snapshot
    with Writer(output, **config) as writer:
        if source_path.is_file():
            # Single file - add directly
            writer.add_file(str(source_path))
        elif source_path.is_dir():
            # Directory - walk recursively and add all files
            for root, dirs, files in os.walk(source_path):
                dirs.sort()  # Ensure deterministic traversal order
                files.sort()
                for file in files:
                    file_path = Path(root) / file
                    writer.add_file(str(file_path))
        else:
            raise ValidationError(f"Source not found: {source}")

    # Return metadata
    return inspect(output)


__all__ = ["PROFILES", "build"]
