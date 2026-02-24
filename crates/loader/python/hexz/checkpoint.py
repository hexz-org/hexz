"""Tensor-aware checkpoint save/load for Hexz.

Provides a 2-line API for saving and loading PyTorch state_dicts as Hexz
snapshots with cross-version deduplication, random-access tensor loading,
and support for all common dtypes including bfloat16.

Example:
    >>> import torch, hexz.checkpoint as ckpt
    >>> state = {"weight": torch.randn(4096, 4096), "bias": torch.zeros(4096)}
    >>> ckpt.save(state, "model_v1.hxz")
    >>> restored = ckpt.load("model_v1.hxz")
    >>> torch.allclose(state["weight"], restored["weight"])
    True

    >>> # Cross-version dedup: only changed tensors take space
    >>> state["bias"] = torch.ones(4096)
    >>> ckpt.save(state, "model_v2.hxz", parent="model_v1.hxz")
"""

from typing import Any, Dict, List, Literal, Optional

from .exceptions import FormatError, ValidationError
from .typing import PathLike
from .utils import Metadata, inspect
from .writer import Writer
from .reader import Reader

_CHECKPOINT_VERSION = "1.0"

# Dtype string <-> torch dtype mapping. Populated lazily on first use.
_DTYPE_MAP: Optional[Dict[str, Any]] = None
_DTYPE_REVERSE: Optional[Dict[Any, str]] = None


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
    global _DTYPE_MAP, _DTYPE_REVERSE
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


def _dtype_to_str(dtype) -> str:
    """Convert a torch dtype to its manifest string."""
    _build_dtype_maps()
    assert _DTYPE_REVERSE is not None
    name = _DTYPE_REVERSE.get(dtype)
    if name is None:
        raise ValidationError(f"Unsupported tensor dtype: {dtype}")
    return name


def _str_to_torch_dtype(name: str):
    """Convert a manifest dtype string to a torch dtype."""
    _build_dtype_maps()
    assert _DTYPE_MAP is not None
    dtype = _DTYPE_MAP.get(name)
    if dtype is None:
        raise FormatError(f"Unknown dtype in checkpoint manifest: {name!r}")
    return dtype


def _tensor_to_bytes(tensor) -> bytes:
    """Convert a single tensor to raw bytes."""
    torch = _ensure_torch()

    t = tensor.detach().cpu().contiguous()
    if t.dtype == torch.bfloat16:
        return t.view(torch.uint16).numpy().tobytes()
    else:
        return t.numpy().tobytes()


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


def _classify_value(key: str, value) -> str:
    """Classify a state_dict value as 'tensor', 'scalar', or raise."""
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
    if isinstance(value, bool):
        return "bool"
    if isinstance(value, int):
        return "int"
    if isinstance(value, float):
        return "float"
    if isinstance(value, str):
        return "str"
    raise ValidationError(f"Not a scalar: {type(value).__name__}")


def save(
    state_dict: Dict[str, Any],
    path: PathLike,
    *,
    compression: Literal["lz4", "zstd"] = "zstd",
    block_size: int = 128 * 1024,
    parent: Optional[PathLike] = None,
) -> Metadata:
    """Save a PyTorch state_dict as a Hexz checkpoint.

    Args:
        state_dict: Dictionary mapping names to tensors and scalars.
        path: Output .hxz file path.
        compression: Compression algorithm ("lz4" or "zstd").
        block_size: Block size in bytes.
        parent: Path to parent checkpoint for cross-version deduplication.

    Returns:
        Metadata for the created checkpoint.

    Raises:
        ValidationError: If state_dict contains unsupported types or dtypes.
        ImportError: If PyTorch is not installed.

    Example:
        >>> import torch, hexz.checkpoint as ckpt
        >>> sd = {"w": torch.randn(1024, 1024), "step": 100}
        >>> meta = ckpt.save(sd, "ckpt.hxz")
    """
    _ensure_torch()

    # Classify all values up front
    tensors_keys = []
    scalars_dict: Dict[str, Dict[str, Any]] = {}

    for key in sorted(state_dict.keys()):
        value = state_dict[key]
        kind = _classify_value(key, value)
        if kind == "tensor":
            # Validate dtype early
            _dtype_to_str(value.dtype)
            tensors_keys.append(key)
        else:
            scalars_dict[key] = {
                "type": _scalar_type_name(value),
                "value": value,
            }

    # Write tensors
    tensors_manifest: Dict[str, Dict[str, Any]] = {}
    offset = 0

    with Writer(
        path,
        compression=compression,
        block_size=block_size,
        dedup=True,
        parent=parent,
    ) as writer:
        for name in tensors_keys:
            tensor = state_dict[name]
            data = _tensor_to_bytes(tensor)
            writer.add_bytes(data)
            tensors_manifest[name] = {
                "offset": offset,
                "length": len(data),
                "dtype": _dtype_to_str(tensor.dtype),
                "shape": list(tensor.shape),
            }
            offset += len(data)

        manifest = {
            "hexz_checkpoint": _CHECKPOINT_VERSION,
            "tensor_count": len(tensors_keys),
            "tensors": tensors_manifest,
            "scalars": scalars_dict,
        }
        writer.add_metadata(manifest)

    return inspect(path)


def load(
    path: PathLike,
    *,
    keys: Optional[List[str]] = None,
    device: str = "cpu",
) -> Dict[str, Any]:
    """Load tensors and scalars from a Hexz checkpoint.

    Args:
        path: Path to .hxz checkpoint file.
        keys: If provided, only load these keys (tensors and/or scalars).
            Loads all keys if None.
        device: Target device for tensors (e.g. "cpu", "cuda:0").

    Returns:
        Dictionary mapping names to tensors and scalar values.

    Raises:
        FormatError: If the file is not a Hexz checkpoint.
        ValidationError: If a requested key does not exist in the checkpoint.
        ImportError: If PyTorch is not installed.

    Example:
        >>> sd = ckpt.load("ckpt.hxz")
        >>> sd = ckpt.load("ckpt.hxz", keys=["model.weight"], device="cuda:0")
    """
    _ensure_torch()

    meta = inspect(path)
    try:
        meta["hexz_checkpoint"]
    except KeyError:
        raise FormatError(
            "Not a Hexz checkpoint (missing 'hexz_checkpoint' marker). "
            "Use hexz.Reader for regular snapshots."
        )

    tensors_info = meta["tensors"]
    try:
        scalars_info = meta["scalars"]
    except KeyError:
        scalars_info = {}
    all_keys = set(tensors_info.keys()) | set(scalars_info.keys())

    # Determine which keys to load
    if keys is not None:
        for k in keys:
            if k not in all_keys:
                raise ValidationError(
                    f"Key {k!r} not found in checkpoint. "
                    f"Available keys: {sorted(all_keys)}"
                )
        load_keys = set(keys)
    else:
        load_keys = all_keys

    result: Dict[str, Any] = {}

    # Load tensors via random-access reads
    tensor_keys_to_load = [k for k in sorted(tensors_info.keys()) if k in load_keys]
    if tensor_keys_to_load:
        with Reader(str(path)) as reader:
            for name in tensor_keys_to_load:
                info = tensors_info[name]
                data: bytes = reader.read(info["length"], offset=info["offset"])  # type: ignore[assignment]
                result[name] = _bytes_to_tensor(
                    data, info["dtype"], info["shape"], device
                )

    # Load scalars
    for name in sorted(scalars_info.keys()):
        if name not in load_keys:
            continue
        scalar = scalars_info[name]
        value = scalar["value"]
        stype = scalar["type"]
        # Reconstruct Python type (JSON may have coerced bool→int, etc.)
        if stype == "bool":
            value = bool(value)
        elif stype == "int":
            value = int(value)
        elif stype == "float":
            value = float(value)
        elif stype == "str":
            value = str(value)
        result[name] = value

    return result


def manifest(path: PathLike) -> Dict[str, Dict[str, Any]]:
    """Read tensor metadata from a checkpoint without loading data.

    Args:
        path: Path to .hxz checkpoint file.

    Returns:
        Dictionary mapping tensor names to their metadata
        (offset, length, dtype, shape).

    Raises:
        FormatError: If the file is not a Hexz checkpoint.

    Example:
        >>> info = ckpt.manifest("ckpt.hxz")
        >>> for name, meta in info.items():
        ...     print(f"{name}: {meta['dtype']} {meta['shape']}")
    """
    meta = inspect(path)
    try:
        meta["hexz_checkpoint"]
    except KeyError:
        raise FormatError("Not a Hexz checkpoint (missing 'hexz_checkpoint' marker).")

    return meta["tensors"]


__all__ = ["save", "load", "manifest"]
