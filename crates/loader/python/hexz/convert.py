"""Data format conversion utilities for Hexz.

This module provides functions to convert external data formats (tar, HDF5,
WebDataset) into Hexz snapshots. It supports auto-detection of input formats
from file extensions and delegates to the Writer for actual snapshot creation.

Supported formats:
- tar: Standard tar archives (.tar, .tar.gz, .tgz, .tar.bz2)
- hdf5: HDF5 files (.h5, .hdf5) — requires h5py
- webdataset: WebDataset archives (.wds) — tar-based with sample grouping

Example:
    >>> import hexz
    >>> meta = hexz.convert("dataset.tar.gz", "dataset.hxz")
    >>> print(meta)
    >>>
    >>> # With explicit format and compression
    >>> meta = hexz.convert("data.bin", "data.hxz", format="tar", compression="zstd")
    >>>
    >>> # Using a build profile
    >>> meta = hexz.convert("data.h5", "data.hxz", profile="ml")
"""

import os
import tarfile
from pathlib import Path
from typing import Any, Dict, Optional

from .exceptions import ValidationError
from .profiles import PROFILES
from .typing import PathLike
from .utils import Metadata, inspect
from .writer import Writer

# Extension → format mapping for auto-detection
_EXTENSION_MAP = {
    ".tar": "tar",
    ".tar.gz": "tar",
    ".tgz": "tar",
    ".tar.bz2": "tar",
    ".tar.xz": "tar",
    ".h5": "hdf5",
    ".hdf5": "hdf5",
    ".wds": "webdataset",
}


def _detect_format(path: str) -> str:
    """Detect input format from file extension.

    Args:
        path: Input file path.

    Returns:
        Format string: "tar", "hdf5", or "webdataset".

    Raises:
        ValidationError: If the extension is not recognized.
    """
    name = Path(path).name.lower()

    # Check compound extensions first (e.g. .tar.gz)
    for ext, fmt in sorted(_EXTENSION_MAP.items(), key=lambda x: -len(x[0])):
        if name.endswith(ext):
            return fmt

    raise ValidationError(
        f"Cannot detect format from extension: {Path(path).suffix!r}. "
        f"Supported extensions: {', '.join(sorted(_EXTENSION_MAP.keys()))}. "
        f"Use format= to specify explicitly."
    )


def _build_writer_kwargs(
    compression: str,
    block_size: Optional[int],
    profile: Optional[str],
    extra: Dict[str, Any],
) -> Dict[str, Any]:
    """Build keyword arguments for the Writer constructor.

    If a profile is specified, its settings are used as defaults and can be
    overridden by explicit parameters.
    """
    kwargs: Dict[str, Any] = {}

    if profile is not None:
        if profile not in PROFILES:
            raise ValidationError(
                f"Unknown profile: {profile!r}. Available: {', '.join(PROFILES.keys())}"
            )
        kwargs.update(PROFILES[profile])

    # Explicit parameters override profile defaults
    kwargs["compression"] = compression
    if block_size is not None:
        kwargs["block_size"] = block_size

    # Pass through any extra kwargs (e.g. dedup, cdc)
    kwargs.update(extra)

    return kwargs


def _convert_tar(
    input_path: str,
    output_path: str,
    writer_kwargs: Dict[str, Any],
) -> Dict[str, Any]:
    """Convert a tar archive to a Hexz snapshot.

    Streams each tar member through Writer.add_bytes() without extracting
    to disk. Stores a file manifest in snapshot metadata.

    Returns:
        Source metadata dict for embedding in the snapshot.
    """
    source_files = []
    total_bytes = 0

    with Writer(output_path, **writer_kwargs) as writer:
        with tarfile.open(input_path, "r:*") as tf:
            for member in tf:
                if not member.isfile():
                    continue

                fileobj = tf.extractfile(member)
                if fileobj is None:
                    continue

                data = fileobj.read()
                writer.add_bytes(data)

                source_files.append(
                    {
                        "name": member.name,
                        "size": member.size,
                        "offset": total_bytes,
                    }
                )
                total_bytes += len(data)

        source_meta = {
            "source": {
                "format": "tar",
                "original_path": os.path.basename(input_path),
                "total_files": len(source_files),
                "total_bytes": total_bytes,
                "source_files": source_files,
            }
        }
        writer.add_metadata(source_meta)

    return source_meta


def _convert_hdf5(
    input_path: str,
    output_path: str,
    writer_kwargs: Dict[str, Any],
) -> Dict[str, Any]:
    """Convert an HDF5 file to a Hexz snapshot.

    Walks all datasets via h5py.File.visititems(), converts each to bytes,
    and stores dataset paths/shapes/dtypes in metadata.

    Returns:
        Source metadata dict for embedding in the snapshot.
    """
    try:
        import h5py
    except ImportError:
        raise ImportError(
            "h5py is required for HDF5 conversion. Install with: pip install hexz[hdf5]"
        )

    datasets_info = []
    total_bytes = 0

    with Writer(output_path, **writer_kwargs) as writer:
        with h5py.File(input_path, "r") as f:
            items = []

            def _collect(name, obj):
                if isinstance(obj, h5py.Dataset):
                    items.append((name, obj))

            f.visititems(_collect)

            for name, dataset in items:
                data = dataset[()].tobytes()
                writer.add_bytes(data)

                datasets_info.append(
                    {
                        "path": name,
                        "shape": list(dataset.shape),
                        "dtype": str(dataset.dtype),
                        "size": len(data),
                        "offset": total_bytes,
                    }
                )
                total_bytes += len(data)

        source_meta = {
            "source": {
                "format": "hdf5",
                "original_path": os.path.basename(input_path),
                "total_datasets": len(datasets_info),
                "total_bytes": total_bytes,
                "datasets": datasets_info,
            }
        }
        writer.add_metadata(source_meta)

    return source_meta


def _convert_webdataset(
    input_path: str,
    output_path: str,
    writer_kwargs: Dict[str, Any],
) -> Dict[str, Any]:
    """Convert a WebDataset archive to a Hexz snapshot.

    WebDataset is tar-based. Members are grouped by sample key (the filename
    stem before the first dot). Stores sample-grouped manifest in metadata.

    Returns:
        Source metadata dict for embedding in the snapshot.
    """
    samples: Dict[str, list] = {}
    total_bytes = 0

    with Writer(output_path, **writer_kwargs) as writer:
        with tarfile.open(input_path, "r:*") as tf:
            for member in tf:
                if not member.isfile():
                    continue

                fileobj = tf.extractfile(member)
                if fileobj is None:
                    continue

                data = fileobj.read()
                writer.add_bytes(data)

                # Group by sample key (stem before first dot)
                basename = os.path.basename(member.name)
                sample_key = basename.split(".")[0] if "." in basename else basename

                if sample_key not in samples:
                    samples[sample_key] = []

                samples[sample_key].append(
                    {
                        "name": member.name,
                        "extension": "." + ".".join(basename.split(".")[1:])
                        if "." in basename
                        else "",
                        "size": member.size,
                        "offset": total_bytes,
                    }
                )
                total_bytes += len(data)

        source_meta = {
            "source": {
                "format": "webdataset",
                "original_path": os.path.basename(input_path),
                "total_samples": len(samples),
                "total_files": sum(len(v) for v in samples.values()),
                "total_bytes": total_bytes,
                "samples": samples,
            }
        }
        writer.add_metadata(source_meta)

    return source_meta


# Dispatch table
_CONVERTERS = {
    "tar": _convert_tar,
    "hdf5": _convert_hdf5,
    "webdataset": _convert_webdataset,
}


def convert(
    input: PathLike,
    output: PathLike,
    *,
    format: Optional[str] = None,
    compression: str = "lz4",
    block_size: Optional[int] = None,
    profile: Optional[str] = None,
    **kwargs: Any,
) -> Metadata:
    """Convert an external data format into a Hexz snapshot.

    Supports tar, HDF5, and WebDataset formats. The format is auto-detected
    from the file extension unless explicitly specified.

    Args:
        input: Path to the input file.
        output: Path to the output .hxz snapshot.
        format: Source format ("tar", "hdf5", "webdataset"). Auto-detected if None.
        compression: Compression algorithm ("lz4" or "zstd"). Default: "lz4".
        block_size: Block size in bytes. Uses profile/default if None.
        profile: Build profile ("ml", "eda", "embedded", "generic", "archival").
        **kwargs: Additional Writer options (e.g. dedup, cdc).

    Returns:
        Metadata object with snapshot information.

    Raises:
        ValidationError: If the format is unknown or the input file is invalid.
        FileNotFoundError: If the input file does not exist.
        ImportError: If h5py is required but not installed.

    Example:
        >>> import hexz
        >>> meta = hexz.convert("data.tar.gz", "data.hxz")
        >>> print(f"Compressed to {meta.size_compressed:,} bytes")
        >>>
        >>> # HDF5 with ML profile
        >>> meta = hexz.convert("features.h5", "features.hxz", profile="ml")
        >>>
        >>> # Explicit format override
        >>> meta = hexz.convert("data.bin", "data.hxz", format="tar")
    """
    input_str = str(input)
    output_str = str(output)

    # Validate input exists
    if not os.path.isfile(input_str):
        raise FileNotFoundError(f"Input file not found: {input_str}")

    # Detect or validate format
    if format is None:
        fmt = _detect_format(input_str)
    else:
        fmt = format.lower()
        if fmt not in _CONVERTERS:
            raise ValidationError(
                f"Unknown format: {fmt!r}. "
                f"Supported formats: {', '.join(sorted(_CONVERTERS.keys()))}"
            )

    # Build writer kwargs from profile + explicit params
    writer_kwargs = _build_writer_kwargs(compression, block_size, profile, kwargs)

    # Dispatch to format-specific converter
    converter = _CONVERTERS[fmt]
    converter(input_str, output_str, writer_kwargs)

    # Return metadata for the created snapshot
    return inspect(output_str)


__all__ = ["convert"]
