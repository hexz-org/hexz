"""NumPy array integration for Strata.

This module provides utilities for reading and writing NumPy arrays
to/from Strata snapshots with zero-copy support where possible.
"""

from typing import Optional, Tuple, Union
from pathlib import Path
import warnings

from .typing import PathLike, Shape
from .reader import Reader
from .writer import Writer
from .exceptions import ValidationError

try:
    import numpy as np

    HAS_NUMPY = True
except ImportError:
    HAS_NUMPY = False
    np = None  # type: ignore


def _check_numpy():
    """Check if NumPy is available."""
    if not HAS_NUMPY:
        raise ImportError(
            "NumPy is required for array operations. "
            "Install it with: pip install numpy"
        )


def read_array(
    source: Union[PathLike, Reader],
    *,
    offset: int = 0,
    shape: Optional[Shape] = None,
    dtype: Union[str, "np.dtype"] = "float32",
    order: str = "C",
    copy: bool = True,
) -> "np.ndarray":
    """Read NumPy array from strata file.

    Args:
        source: Path to .st file or Reader instance
        offset: Byte offset in file
        shape: Array shape (required if reading from raw offset)
        dtype: NumPy dtype (string or numpy.dtype)
        order: 'C' (row-major) or 'F' (column-major)
        copy: Whether to copy data (True) or return view (False)

    Returns:
        NumPy array

    Example:
        >>> array = strata.read_array(
        ...     "data.st",
        ...     offset=0,
        ...     shape=(1000, 784),
        ...     dtype='float32'
        ... )
        >>> print(array.shape)
        (1000, 784)
    """
    _check_numpy()

    if shape is None:
        raise ValidationError("shape parameter is required")

    # Convert dtype to numpy dtype
    dtype = np.dtype(dtype)

    # Calculate size
    size = int(dtype.itemsize * np.prod(shape))

    # Open reader if needed
    if isinstance(source, (str, Path)):
        reader = Reader(str(source))
        should_close = True
    else:
        reader = source
        should_close = False

    try:
        # Read data at offset
        data = reader.read_at(offset, size)

        # Create array from buffer
        arr = np.frombuffer(data, dtype=dtype)

        # Reshape with specified order
        if order == "C":
            arr = arr.reshape(shape, order="C")
        elif order == "F":
            arr = arr.reshape(shape, order="F")
        else:
            raise ValidationError(f"Invalid order: {order}. Use 'C' or 'F'.")

        # Copy if requested (default)
        if copy:
            return arr.copy()
        else:
            # Warning about immutability of view
            warnings.warn(
                "Returning a view into read-only buffer. " "Array will be immutable.",
                UserWarning,
            )
            return arr

    finally:
        if should_close:
            reader.close()


def write_array(
    dest: Union[PathLike, Writer],
    array: "np.ndarray",
    *,
    offset: int = 0,
    compression: str = "lz4",
) -> int:
    """Write NumPy array to strata file.

    Args:
        dest: Path to .st file or Writer instance
        array: NumPy array to write
        offset: Byte offset to write at
        compression: Compression algorithm ('lz4' or 'zstd')

    Returns:
        Number of bytes written

    Example:
        >>> import numpy as np
        >>> data = np.random.rand(1000, 784).astype('float32')
        >>> strata.write_array("output.st", data)
    """
    _check_numpy()

    # Ensure contiguous array
    if not array.flags["C_CONTIGUOUS"]:
        warnings.warn(
            "Array is not C-contiguous, making a copy. "
            "This may use additional memory.",
            UserWarning,
        )
        array = np.ascontiguousarray(array)

    # Convert to bytes
    data_bytes = array.tobytes()

    # Open writer if needed
    if isinstance(dest, (str, Path)):
        writer = Writer(str(dest), compression=compression)
        should_close = True
    else:
        writer = dest
        should_close = False

    try:
        # Write bytes at offset
        bytes_written = writer.write(data_bytes, offset=offset)
        return bytes_written
    finally:
        if should_close:
            writer.finalize()


class ArrayView:
    """NumPy memmap-like interface for strata files.

    Provides random access to array data stored in a Strata snapshot
    without loading the entire array into memory.

    Example:
        >>> view = strata.ArrayView(
        ...     "data.st",
        ...     shape=(10000, 784),
        ...     dtype='float32'
        ... )
        >>> # Read a slice
        >>> batch = view[0:100]  # Returns rows 0-99
        >>> print(batch.shape)
        (100, 784)
    """

    def __init__(
        self,
        path: PathLike,
        shape: Shape,
        dtype: Union[str, "np.dtype"] = "float32",
        offset: int = 0,
    ):
        """Create an array view into a Strata file.

        Args:
            path: Path to .st file
            shape: Shape of the array
            dtype: NumPy dtype
            offset: Byte offset where array data starts
        """
        _check_numpy()

        self._reader = Reader(str(path))
        self._shape = shape
        self._dtype = np.dtype(dtype)
        self._offset = offset
        self._itemsize = self._dtype.itemsize

    @property
    def shape(self) -> Shape:
        """Array shape."""
        return self._shape

    @property
    def dtype(self) -> "np.dtype":
        """Array dtype."""
        return self._dtype

    @property
    def ndim(self) -> int:
        """Number of dimensions."""
        return len(self._shape)

    @property
    def size(self) -> int:
        """Total number of elements."""
        return int(np.prod(self._shape))

    def __len__(self) -> int:
        """Length of first dimension."""
        return self._shape[0] if self._shape else 0

    def __getitem__(self, key) -> "np.ndarray":
        """Support slicing: view[0:10] or view[0:10, :]

        Args:
            key: Integer, slice, or tuple of slices

        Returns:
            NumPy array containing the requested data
        """
        # Handle single integer index
        if isinstance(key, int):
            # Read a single row (for 2D arrays)
            if self.ndim == 1:
                # 1D array: read single element
                byte_offset = self._offset + key * self._itemsize
                data = self._reader.read_at(byte_offset, self._itemsize)
                return np.frombuffer(data, dtype=self._dtype)[0]
            else:
                # Multi-D array: read single row
                row_size = int(np.prod(self._shape[1:])) * self._itemsize
                byte_offset = self._offset + key * row_size
                data = self._reader.read_at(byte_offset, row_size)
                arr = np.frombuffer(data, dtype=self._dtype)
                return arr.reshape(self._shape[1:])

        # Handle slice
        elif isinstance(key, slice):
            # For 1D arrays
            if self.ndim == 1:
                start, stop, step = key.indices(self._shape[0])
                if step != 1:
                    raise NotImplementedError("Non-unit strides not yet supported")

                num_elements = stop - start
                byte_offset = self._offset + start * self._itemsize
                byte_size = num_elements * self._itemsize

                data = self._reader.read_at(byte_offset, byte_size)
                return np.frombuffer(data, dtype=self._dtype)

            # For 2D+ arrays, read rows
            else:
                start, stop, step = key.indices(self._shape[0])
                if step != 1:
                    raise NotImplementedError("Non-unit strides not yet supported")

                num_rows = stop - start
                row_size = int(np.prod(self._shape[1:])) * self._itemsize
                byte_offset = self._offset + start * row_size
                byte_size = num_rows * row_size

                data = self._reader.read_at(byte_offset, byte_size)
                arr = np.frombuffer(data, dtype=self._dtype)
                result_shape = (num_rows,) + self._shape[1:]
                return arr.reshape(result_shape)

        # Handle tuple of slices (multi-dimensional slicing)
        elif isinstance(key, tuple):
            # For now, only support slicing on first dimension
            if len(key) == 0:
                raise ValidationError("Empty index")

            first_key = key[0]
            rest_keys = key[1:]

            # Get data for first dimension
            data = self.__getitem__(first_key)

            # Apply remaining indices
            if rest_keys:
                return data[rest_keys]
            return data

        else:
            raise TypeError(f"Unsupported index type: {type(key)}")

    def __enter__(self):
        """Context manager entry."""
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit."""
        self.close()

    def close(self):
        """Close the underlying reader."""
        self._reader.close()

    def __repr__(self) -> str:
        return f"ArrayView(shape={self.shape}, dtype={self.dtype})"


# Export all public APIs
__all__ = [
    "read_array",
    "write_array",
    "ArrayView",
]
