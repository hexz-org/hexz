"""Internal helpers for hexz. Not part of the public API."""

import os
import re
import tarfile
from pathlib import Path
from typing import Any, Dict, List, Optional

# ---------------------------------------------------------------------------
# Checkpoint helpers (from checkpoint.py)
# ---------------------------------------------------------------------------

_CHECKPOINT_VERSION = "1.0"

# Dtype string <-> torch dtype mapping. Populated lazily on first use.
_DTYPE_MAP: Optional[Dict[str, Any]] = None
_DTYPE_REVERSE: Optional[Dict[Any, str]] = None

# Dtype -> element size in bytes (for byte-shuffle). Populated lazily.
_DTYPE_SIZES: Optional[Dict[Any, int]] = None


def _ensure_torch():
    """Lazy-import torch, raising a helpful error if missing."""
    try:
        import torch

        return torch
    except ImportError:
        raise ImportError(
            "PyTorch is required for hexz.checkpoint. Install with: pip install torch"
        )


def _build_dtype_maps():
    """Build dtype string <-> torch.dtype mappings (once)."""
    global _DTYPE_MAP, _DTYPE_REVERSE, _DTYPE_SIZES
    if _DTYPE_MAP is not None:
        return

    torch = _ensure_torch()
    _DTYPE_MAP = {
        "float16": torch.float16,
        "float32": torch.float32,
        "float64": torch.float64,
        "bfloat16": torch.bfloat16,
        "int8": torch.int8,
        "int16": torch.int16,
        "int32": torch.int32,
        "int64": torch.int64,
        "uint8": torch.uint8,
        "bool": torch.bool,
    }
    _DTYPE_REVERSE = {v: k for k, v in _DTYPE_MAP.items()}
    _DTYPE_SIZES = {
        torch.float16: 2,
        torch.bfloat16: 2,
        torch.float32: 4,
        torch.float64: 8,
        torch.int8: 1,
        torch.uint8: 1,
        torch.int16: 2,
        torch.int32: 4,
        torch.int64: 8,
        torch.bool: 1,
    }


def _dtype_to_str(dtype) -> str:
    """Convert a torch dtype to its manifest string."""
    from .exceptions import ValidationError

    _build_dtype_maps()
    assert _DTYPE_REVERSE is not None
    name = _DTYPE_REVERSE.get(dtype)
    if name is None:
        raise ValidationError(f"Unsupported tensor dtype: {dtype}")
    return name


def _str_to_torch_dtype(name: str):
    """Convert a manifest dtype string to a torch dtype."""
    from .exceptions import FormatError

    _build_dtype_maps()
    assert _DTYPE_MAP is not None
    dtype = _DTYPE_MAP.get(name)
    if dtype is None:
        raise FormatError(f"Unknown dtype in checkpoint manifest: {name!r}")
    return dtype


def _tensor_to_buffer(tensor):
    """Return a zero-copy memoryview of tensor memory (no Python-side copy)."""
    torch = _ensure_torch()

    t = tensor.detach().cpu().contiguous()
    if t.dtype == torch.bfloat16:
        return memoryview(t.view(torch.uint16).numpy())
    return memoryview(t.numpy())


def _bytes_to_tensor(data: bytes, dtype_str: str, shape: List[int], device: str):
    """Reconstruct a tensor from raw bytes."""
    import numpy as np

    torch = _ensure_torch()

    # Validate dtype string
    _str_to_torch_dtype(dtype_str)

    if dtype_str == "bfloat16":
        t = torch.frombuffer(bytearray(data), dtype=torch.bfloat16)
        return (t.reshape(shape) if shape else t.reshape(())).to(device)

    # Map torch dtype name to numpy dtype
    _np_dtype_map = {
        "float16": np.float16,
        "float32": np.float32,
        "float64": np.float64,
        "int8": np.int8,
        "int16": np.int16,
        "int32": np.int32,
        "int64": np.int64,
        "uint8": np.uint8,
        "bool": np.bool_,
    }
    np_dtype = _np_dtype_map[dtype_str]
    arr = np.frombuffer(data, dtype=np_dtype)
    arr = arr.reshape(shape) if shape else arr.reshape(())
    return torch.from_numpy(arr.copy()).to(device)


def _byte_unshuffle(data: bytes, element_size: int) -> bytes:
    """Byte-unshuffle: inverse of byte_shuffle used during save."""
    if element_size <= 1 or len(data) < element_size:
        return data
    import numpy as np

    n = len(data)
    count = n // element_size
    tail = n % element_size
    main_len = count * element_size
    arr = np.frombuffer(data, dtype=np.uint8, count=main_len)
    unshuffled = arr.reshape(element_size, count).T.ravel()
    if tail > 0:
        tail_arr = np.frombuffer(data, dtype=np.uint8, offset=main_len)
        return unshuffled.tobytes() + tail_arr.tobytes()
    return unshuffled.tobytes()


def _xor_bytes(a: bytes, b: bytes) -> bytes:
    """XOR two byte strings of equal length."""
    import numpy as np

    return np.bitwise_xor(
        np.frombuffer(a, dtype=np.uint8),
        np.frombuffer(b, dtype=np.uint8),
    ).tobytes()


def _classify_value(key: str, value) -> str:
    """Classify a state_dict value as 'tensor', 'scalar', or raise."""
    from .exceptions import ValidationError

    torch = _ensure_torch()
    if isinstance(value, torch.Tensor):
        return "tensor"
    if isinstance(value, (int, float, bool, str)):
        return "scalar"
    raise ValidationError(
        f"state_dict key {key!r}: unsupported type {type(value).__name__}. "
        f"Expected Tensor, int, float, bool, or str."
    )


def _scalar_type_name(value) -> str:
    """Return the manifest type name for a scalar value."""
    from .exceptions import ValidationError

    if isinstance(value, bool):
        return "bool"
    if isinstance(value, int):
        return "int"
    if isinstance(value, float):
        return "float"
    if isinstance(value, str):
        return "str"
    raise ValidationError(f"Not a scalar: {type(value).__name__}")


# ---------------------------------------------------------------------------
# Reader helpers (from reader.py)
# ---------------------------------------------------------------------------


def _parse_cache_size(size_str: str) -> int:
    """Parse a cache size string like '512M', '1G', '2GB' into bytes."""
    size_str = size_str.strip().upper()

    # Match number followed by optional unit
    match = re.match(r"^(\d+(?:\.\d+)?)\s*([KMGT]I?B?)?$", size_str)
    if not match:
        raise ValueError(f"Invalid cache size format: {size_str}")

    number_str, unit = match.groups()
    number = float(number_str)

    # Parse unit
    if not unit:
        return int(number)

    # Normalize unit (remove 'I' and 'B' variations)
    unit = unit.replace("I", "").replace("B", "")

    multipliers = {
        "K": 1024,
        "M": 1024**2,
        "G": 1024**3,
        "T": 1024**4,
    }

    if unit not in multipliers:
        raise ValueError(f"Unknown unit in cache size: {unit}")

    return int(number * multipliers[unit])


# ---------------------------------------------------------------------------
# Writer helpers (from writer.py)
# ---------------------------------------------------------------------------

_COMPRESSION_LEVELS = {
    "fast": {"lz4": None, "zstd": 1},
    "balanced": {"lz4": None, "zstd": 3},
    "tight": {"lz4": None, "zstd": 9},
}

# ---------------------------------------------------------------------------
# Convert helpers (from convert.py)
# ---------------------------------------------------------------------------

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
    """Detect input format from file extension."""
    from .exceptions import ValidationError

    name = Path(path).name.lower()

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
    """Build keyword arguments for the Writer constructor."""
    from .exceptions import ValidationError
    from .profiles import PROFILES

    kwargs: Dict[str, Any] = {}

    if profile is not None:
        if profile not in PROFILES:
            raise ValidationError(
                f"Unknown profile: {profile!r}. Available: {', '.join(PROFILES.keys())}"
            )
        kwargs.update(PROFILES[profile])

    kwargs["compression"] = compression
    if block_size is not None:
        kwargs["block_size"] = block_size

    kwargs.update(extra)

    return kwargs


def _convert_tar(
    input_path: str,
    output_path: str,
    writer_kwargs: Dict[str, Any],
) -> Dict[str, Any]:
    """Convert a tar archive to a Hexz snapshot."""
    from .writer import Writer

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
    """Convert an HDF5 file to a Hexz snapshot."""
    try:
        import h5py
    except ImportError:
        raise ImportError(
            "h5py is required for HDF5 conversion. Install with: pip install hexz[hdf5]"
        )

    from .writer import Writer

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
    """Convert a WebDataset archive to a Hexz snapshot."""
    from .writer import Writer

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


_CONVERTERS = {
    "tar": _convert_tar,
    "hdf5": _convert_hdf5,
    "webdataset": _convert_webdataset,
}

# ---------------------------------------------------------------------------
# Array helpers (from array.py)
# ---------------------------------------------------------------------------

try:
    import numpy as _np

    HAS_NUMPY = True
except ImportError:
    HAS_NUMPY = False
    _np = None  # type: ignore


def _check_numpy():
    """Check if NumPy is available."""
    if not HAS_NUMPY:
        raise ImportError(
            "NumPy is required for array operations. Install it with: pip install numpy"
        )
