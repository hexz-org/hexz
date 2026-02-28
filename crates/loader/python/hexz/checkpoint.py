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
from ._internal import (
    _CHECKPOINT_VERSION,
    _DTYPE_SIZES,
    _ensure_torch,
    _dtype_to_str,
    _tensor_to_buffer,
    _bytes_to_tensor,
    _classify_value,
    _scalar_type_name,
)


def save(
    state_dict: Dict[str, Any],
    path: PathLike,
    *,
    compression: Literal["lz4", "zstd"] = "zstd",
    block_size: int = 128 * 1024,
    parent: Optional[PathLike] = None,
    base: Optional[Dict[str, Any]] = None,
    num_workers: int = 0,
    progress: bool = False,
    message: Optional[str] = None,
) -> Metadata:
    """Save a PyTorch state_dict as a Hexz checkpoint.

    Args:
        state_dict: Dictionary mapping names to tensors and scalars.
        path: Output .hxz file path.
        compression: Compression algorithm ("lz4" or "zstd").
        block_size: Block size in bytes.
        parent: Path to parent checkpoint for cross-version deduplication.
        base: In-memory state_dict of the parent checkpoint. When provided,
            avoids recursively loading the parent chain for chained XOR deltas.
            Pass the previous step's state_dict here in training loops.
            Important: tensors must be independent copies (use .clone()),
            not views sharing storage with model parameters.
        message: Optional human-readable message stored in the checkpoint
            metadata (e.g. "switched to cosine annealing lr schedule").

    Returns:
        Metadata for the created checkpoint.

    Raises:
        ValidationError: If state_dict contains unsupported types or dtypes.
        ImportError: If PyTorch is not installed.

    Example:
        >>> import torch, hexz.checkpoint as ckpt
        >>> sd = {"w": torch.randn(1024, 1024), "step": 100}
        >>> meta = ckpt.save(sd, "ckpt.hxz", message="initial run")

        >>> # Training loop with chained checkpoints:
        >>> prev_sd = {k: v.clone() for k, v in model.state_dict().items()}
        >>> for step in range(1, 11):
        ...     train_one_step(model)
        ...     new_sd = {k: v.clone() for k, v in model.state_dict().items()}
        ...     ckpt.save(new_sd, f"step_{step}.hxz",
        ...               parent=f"step_{step-1}.hxz", base=prev_sd)
        ...     prev_sd = new_sd
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

    total_bytes = sum(
        state_dict[k].nbytes for k in tensors_keys if hasattr(state_dict[k], "nbytes")
    )

    # Build parent tensor map for XOR delta
    parent_tensors: Optional[Dict[str, Dict[str, Any]]] = None
    if parent is not None:
        try:
            parent_tensors = manifest(parent)
        except Exception:
            parent_tensors = None

    _tqdm = None
    if progress:
        try:
            from tqdm import tqdm as _tqdm
        except ImportError:
            pass

    # Lazily loaded when parent has xor_delta tensors that need reconstruction
    _parent_reconstructed: Optional[Dict[str, Any]] = None

    with Writer(
        path,
        compression=compression,
        block_size=block_size,
        dedup=False,
        parent=parent,
        cdc=False,
        num_workers=num_workers,
    ) as writer:
        pbar = (
            _tqdm(
                total=total_bytes,
                unit="B",
                unit_scale=True,
                unit_divisor=1024,
                desc="saving",
                leave=False,
            )
            if _tqdm is not None
            else None
        )
        for name in tensors_keys:
            tensor = state_dict[name]
            buf = _tensor_to_buffer(tensor)
            length = buf.nbytes

            # Try XOR delta if tensor exists in parent with same byte length
            use_xor = False
            element_size = _DTYPE_SIZES.get(tensor.dtype, 1) if _DTYPE_SIZES else 1
            if parent_tensors is not None and name in parent_tensors:
                pinfo = parent_tensors[name]
                if pinfo["length"] == length:
                    if pinfo.get("storage") == "xor_delta":
                        # Parent tensor is itself a delta — reconstruct the
                        # actual bytes so we XOR against real values, not the
                        # shuffled delta stored in the parent's stream.
                        if _parent_reconstructed is None:
                            _parent_reconstructed = (
                                base if base is not None else load(parent, device="cpu")
                            )
                        parent_buf = _tensor_to_buffer(_parent_reconstructed[name])
                        writer.add_xor_delta_from_buffers(buf, parent_buf, element_size)
                    else:
                        writer.add_xor_delta(
                            buf, pinfo["offset"], pinfo["length"], element_size
                        )
                    use_xor = True

            if not use_xor:
                writer.add_bytes(buf)

            if pbar is not None:
                pbar.set_postfix_str(
                    name.split(".")[-2] if "." in name else name, refresh=False
                )
                pbar.update(length)

            entry: Dict[str, Any] = {
                "offset": offset,
                "length": length,
                "dtype": _dtype_to_str(tensor.dtype),
                "shape": list(tensor.shape),
            }
            if use_xor:
                pinfo = parent_tensors[name]  # type: ignore[index]
                entry["storage"] = "xor_delta"
                entry["base_offset"] = pinfo["offset"]
                entry["base_length"] = pinfo["length"]
                entry["element_size"] = element_size
            else:
                entry["storage"] = "raw"

            tensors_manifest[name] = entry
            offset += length
            # Pad to next block boundary so subsequent tensors stay block-aligned.
            # Zero blocks are stored as 8-byte markers in SnapshotWriter, essentially free.
            pad = (-length) % block_size
            if pad:
                writer.add_bytes(bytes(pad))
                offset += pad

        if pbar is not None:
            pbar.close()

        manifest_data = {
            "hexz_checkpoint": _CHECKPOINT_VERSION,
            "tensor_count": len(tensors_keys),
            "tensors": tensors_manifest,
            "scalars": scalars_dict,
        }
        if message is not None:
            manifest_data["message"] = message
        writer.add_metadata(manifest_data)

    return inspect(path)


def load(
    path: PathLike,
    *,
    keys: Optional[List[str]] = None,
    device: str = "cpu",
    progress: bool = False,
    num_workers: int = 1,
) -> Dict[str, Any]:
    """Load tensors and scalars from a Hexz checkpoint.

    All I/O, decompression, XOR delta reconstruction, and parent chain
    resolution are performed in Rust with the GIL released.

    Args:
        path: Path to .hxz checkpoint file.
        keys: If provided, only load these keys (tensors and/or scalars).
            Loads all keys if None.
        device: Target device for tensors (e.g. "cpu", "cuda:0").
        progress: Reserved for future use.
        num_workers: Reserved for future use (Rust uses rayon internally).

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

    from . import hexz_loader

    # All heavy lifting in Rust (I/O, decompress, XOR delta, unshuffle, rayon parallel)
    try:
        result = hexz_loader.load_checkpoint(str(path), keys=keys)
    except OSError as e:
        msg = str(e)
        if "not found in checkpoint" in msg:
            raise ValidationError(msg) from None
        if "Invalid Format" in msg or "checkpoint" in msg.lower():
            raise FormatError(
                "Not a Hexz checkpoint (missing 'hexz_checkpoint' marker). "
                "Use hexz.Reader for regular snapshots."
            ) from None
        raise

    state_dict: Dict[str, Any] = {}

    # Wrap raw bytes as torch tensors
    for name, raw_bytes in result["tensors"].items():
        meta = result["tensor_meta"][name]
        state_dict[name] = _bytes_to_tensor(
            raw_bytes, meta["dtype"], meta["shape"], device
        )

    # Restore scalars with proper Python types
    for name, scalar in result["scalars"].items():
        value = scalar["value"]
        stype = scalar["type"]
        if stype == "bool":
            value = bool(value)
        elif stype == "int":
            value = int(value)
        elif stype == "float":
            value = float(value)
        elif stype == "str":
            value = str(value)
        state_dict[name] = value

    return state_dict


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


def convert(
    input_path: PathLike,
    output_path: PathLike,
    *,
    base: Optional[PathLike] = None,
    compression: str = "zstd",
    block_size: int = 65536,
) -> dict:
    """Convert a .safetensors file to hexz format.

    No PyTorch required. Reads tensor bytes directly from the source file.
    If base is provided, only changed tensors are stored; frozen tensors
    are referenced from the base without storing bytes.

    Args:
        input_path: Path to the source .safetensors file.
        output_path: Path for the output .hxz file.
        base: Optional path to a parent .hxz file for delta deduplication.
        compression: Compression algorithm ("lz4" or "zstd").
        block_size: Block size in bytes.

    Returns:
        Dict with keys: tensors, total_bytes, stored_bytes, elapsed_secs.

    Example:
        >>> ckpt.convert("model.safetensors", "model.hxz")
        >>> ckpt.convert("ft_v2.safetensors", "ft_v2.hxz", base="ft_v1.hxz")
    """
    from . import hexz_loader

    return hexz_loader.store_safetensors(
        str(input_path),
        str(output_path),
        base=str(base) if base is not None else None,
        compression=compression,
        block_size=block_size,
    )


def extract(
    input_path: PathLike,
    output_path: Optional[PathLike] = None,
    *,
    tensor: Optional[str] = None,
) -> None:
    """Reconstruct a .safetensors file from a hexz checkpoint.

    Args:
        input_path: Path to the .hxz file.
        output_path: Destination .safetensors path. Defaults to the input
            path with the extension replaced by ".safetensors".
        tensor: If provided, write only the raw bytes for this tensor
            (no safetensors header).

    Example:
        >>> ckpt.extract("model.hxz")                   # → model.safetensors
        >>> ckpt.extract("model.hxz", "out.safetensors")
        >>> ckpt.extract("model.hxz", tensor="lm_head.weight")
    """
    from pathlib import Path

    from . import hexz_loader

    if output_path is None:
        output_path = Path(str(input_path)).with_suffix(".safetensors")

    hexz_loader.extract_safetensors(
        str(input_path),
        str(output_path),
        tensor=tensor,
    )


__all__ = ["save", "load", "manifest", "convert", "extract"]


def __dir__():
    return __all__
