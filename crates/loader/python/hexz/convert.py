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
from typing import Any, Optional

from .exceptions import ValidationError
from .typing import PathLike
from .utils import Metadata, inspect
from ._internal import _detect_format, _build_writer_kwargs, _CONVERTERS


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
