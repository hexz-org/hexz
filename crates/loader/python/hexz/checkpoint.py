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
    _byte_unshuffle,
    _xor_bytes,
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

    Args:
        path: Path to .hxz checkpoint file.
        keys: If provided, only load these keys (tensors and/or scalars).
            Loads all keys if None.
        device: Target device for tensors (e.g. "cpu", "cuda:0").
        num_workers: Number of parallel reader threads (default: 1 = sequential).
            Set to 4+ for faster loading from NVMe SSD; each worker opens its
            own file handle and reads+decompresses an independent tensor.

    Returns:
        Dictionary mapping names to tensors and scalar values.

    Raises:
        FormatError: If the file is not a Hexz checkpoint.
        ValidationError: If a requested key does not exist in the checkpoint.
        ImportError: If PyTorch is not installed.

    Example:
        >>> sd = ckpt.load("ckpt.hxz")
        >>> sd = ckpt.load("ckpt.hxz", keys=["model.weight"], device="cuda:0")
        >>> sd = ckpt.load("ckpt.hxz", num_workers=4)  # parallel reads
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
        _tqdm = None
        if progress:
            try:
                from tqdm import tqdm as _tqdm
            except ImportError:
                pass

        total_bytes = sum(tensors_info[k]["length"] for k in tensor_keys_to_load)
        pbar = (
            _tqdm(
                total=total_bytes,
                unit="B",
                unit_scale=True,
                unit_divisor=1024,
                desc="loading",
                leave=False,
            )
            if _tqdm is not None
            else None
        )

        path_str = str(path)

        # Check if any tensor needs XOR delta decoding
        needs_xor = any(
            tensors_info[k].get("storage") == "xor_delta" for k in tensor_keys_to_load
        )
        parent_path = None
        if needs_xor:
            try:
                parent_paths = meta["parent_path"]
            except KeyError:
                parent_paths = []
            if parent_paths:
                parent_path = parent_paths[0]

        # Check parent manifest for chained XOR deltas
        parent_manifest_data = None
        _parent_reconstructed = None
        if parent_path is not None:
            try:
                parent_manifest_data = manifest(parent_path)
            except Exception:
                pass

        # Eagerly reconstruct parent if any tensor is a chained delta
        if parent_manifest_data is not None:
            for _k in tensor_keys_to_load:
                _info = tensors_info[_k]
                if (
                    _info.get("storage") == "xor_delta"
                    and _k in parent_manifest_data
                    and parent_manifest_data[_k].get("storage") == "xor_delta"
                ):
                    _parent_reconstructed = load(parent_path, device="cpu")
                    break

        from . import hexz_loader as _loader

        if num_workers > 1:
            # Each worker opens its own Reader (independent file handle + decompressor).
            # Random-access offsets mean reads are fully independent — no locking needed.
            from concurrent.futures import ThreadPoolExecutor, as_completed

            def _read_tensor(name: str):
                info = tensors_info[name]
                r = _loader.Reader(path_str)
                if info.get("storage") == "xor_delta":
                    parent_is_chained = (
                        parent_manifest_data is not None
                        and name in parent_manifest_data
                        and parent_manifest_data[name].get("storage") == "xor_delta"
                    )
                    if parent_is_chained and _parent_reconstructed is not None:
                        delta = r.read(info["length"], offset=info["offset"])
                        delta = _byte_unshuffle(delta, info.get("element_size", 1))
                        parent_buf = bytes(
                            _tensor_to_buffer(_parent_reconstructed[name])
                        )
                        data = _xor_bytes(delta, parent_buf)
                    elif parent_path is not None:
                        pr = _loader.Reader(parent_path)
                        data = r.read_xor_delta(
                            info["length"],
                            info["offset"],
                            pr,
                            info["base_offset"],
                            info.get("element_size", 1),
                        )
                    else:
                        data = r.read(info["length"], offset=info["offset"])
                else:
                    data = r.read(info["length"], offset=info["offset"])
                return name, _bytes_to_tensor(
                    data, info["dtype"], info["shape"], device
                )

            with ThreadPoolExecutor(max_workers=num_workers) as pool:
                futures = {pool.submit(_read_tensor, n): n for n in tensor_keys_to_load}
                for fut in as_completed(futures):
                    name, tensor = fut.result()
                    result[name] = tensor
                    if pbar is not None:
                        pbar.set_postfix_str(
                            name.split(".")[-2] if "." in name else name, refresh=False
                        )
                        pbar.update(tensors_info[name]["length"])
        else:
            rust_reader = _loader.Reader(path_str)
            parent_reader = (
                _loader.Reader(parent_path) if parent_path is not None else None
            )
            for name in tensor_keys_to_load:
                info = tensors_info[name]
                if info.get("storage") == "xor_delta":
                    parent_is_chained = (
                        parent_manifest_data is not None
                        and name in parent_manifest_data
                        and parent_manifest_data[name].get("storage") == "xor_delta"
                    )
                    if parent_is_chained and _parent_reconstructed is not None:
                        delta: bytes = rust_reader.read(
                            info["length"], offset=info["offset"]
                        )  # type: ignore[assignment]
                        delta = _byte_unshuffle(delta, info.get("element_size", 1))
                        parent_buf = bytes(
                            _tensor_to_buffer(_parent_reconstructed[name])
                        )
                        data: bytes = _xor_bytes(delta, parent_buf)  # type: ignore[assignment]
                    elif parent_reader is not None:
                        data = rust_reader.read_xor_delta(  # type: ignore[assignment]
                            info["length"],
                            info["offset"],
                            parent_reader,
                            info["base_offset"],
                            info.get("element_size", 1),
                        )
                    else:
                        data = rust_reader.read(info["length"], offset=info["offset"])  # type: ignore[assignment]
                else:
                    data = rust_reader.read(info["length"], offset=info["offset"])  # type: ignore[assignment]
                result[name] = _bytes_to_tensor(
                    data, info["dtype"], info["shape"], device
                )
                if pbar is not None:
                    pbar.set_postfix_str(
                        name.split(".")[-2] if "." in name else name, refresh=False
                    )
                    pbar.update(info["length"])

        if pbar is not None:
            pbar.close()

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
