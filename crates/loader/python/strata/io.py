import os
from typing import Union, Tuple, Optional, TYPE_CHECKING
from ._strata_core import StrataReader

if TYPE_CHECKING:
    import numpy as np

# Optional NumPy Support
try:
    import numpy as np

    HAS_NUMPY = True
except ImportError:
    np = None
    HAS_NUMPY = False


def open(
    path: Union[str, os.PathLike],
    s3_region: Optional[str] = None,
    endpoint_url: Optional[str] = None,
    allow_restricted: bool = False,
) -> StrataReader:
    """
    Open a Strata snapshot for reading.

    Supports:
      - Local paths: "/path/to/snap.st"
      - HTTP(S): "http://example.com/snap.st"
      - S3: "s3://my-bucket/my-snap.st"

    Args:
        path: The path or URL to the snapshot.
        s3_region: (Optional) AWS Region for S3 URLs (default: us-east-1).
        endpoint_url: (Optional) Custom S3 endpoint URL (e.g. for MinIO or testing).
        allow_restricted: (Optional) Allow connection to private/internal IPs (default: False).
    """
    return StrataReader(str(path), s3_region, endpoint_url, allow_restricted)


def read_array(
    source: Union[str, StrataReader],
    offset: int,
    shape: Tuple[int, ...],
    dtype: Union[str, "np.dtype"],
    copy: bool = True,
) -> "np.ndarray":
    """
    Read a NumPy array from a Strata snapshot at a specific offset.
    """
    if not HAS_NUMPY:
        raise ImportError("NumPy is not installed. Run `pip install numpy`.")

    dtype = np.dtype(dtype)
    count = 1
    for dim in shape:
        count *= dim
    size_bytes = count * dtype.itemsize

    if isinstance(source, (str, os.PathLike)):
        reader = StrataReader(str(source))
    else:
        reader = source

    data = reader.read_at(offset, size_bytes)

    if len(data) != size_bytes:
        raise IOError(f"Incomplete read: expected {size_bytes} bytes, got {len(data)}")

    arr = np.frombuffer(data, dtype=dtype).reshape(shape)

    if copy:
        return arr.copy()
    return arr
