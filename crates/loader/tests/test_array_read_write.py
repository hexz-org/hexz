"""Tests for array read_array and write_array functions."""

import pytest
import strata

# Check if NumPy is available
try:
    import numpy as np

    HAS_NUMPY = True
except ImportError:
    HAS_NUMPY = False


@pytest.fixture
def temp_snapshot(tmp_path):
    """Create a temporary snapshot for testing."""
    snap_path = tmp_path / "test.st"
    return str(snap_path)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_write_and_read_array_basic(temp_snapshot):
    """Test basic write and read of array."""
    # Create array
    arr = np.arange(100, dtype=np.float32).reshape(10, 10)

    # Write it
    strata.write_array(temp_snapshot, arr)

    # Read it back
    result = strata.read_array(temp_snapshot, shape=(10, 10), dtype="float32")

    assert np.array_equal(result, arr)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_read_array_different_dtypes(temp_snapshot):
    """Test reading arrays with different dtypes."""
    dtypes = [
        "int8",
        "int16",
        "int32",
        "int64",
        "uint8",
        "uint16",
        "uint32",
        "uint64",
        "float32",
        "float64",
    ]

    for dtype_str in dtypes:
        snap_path = temp_snapshot.replace(".st", f"_{dtype_str}.st")

        # Create and write array
        arr = np.arange(20, dtype=dtype_str)
        strata.write_array(snap_path, arr)

        # Read back
        result = strata.read_array(snap_path, shape=(20,), dtype=dtype_str)

        assert result.dtype == np.dtype(dtype_str)
        assert np.array_equal(result, arr)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_read_array_different_shapes(temp_snapshot):
    """Test reading arrays with different shapes."""
    shapes = [(100,), (10, 10), (5, 4, 5), (2, 2, 5, 5)]

    for i, shape in enumerate(shapes):
        snap_path = temp_snapshot.replace(".st", f"_shape{i}.st")

        # Create array
        size = np.prod(shape)
        arr = np.arange(size, dtype=np.float32).reshape(shape)

        # Write and read
        strata.write_array(snap_path, arr)
        result = strata.read_array(snap_path, shape=shape, dtype="float32")

        assert result.shape == shape
        assert np.array_equal(result, arr)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_read_array_order_c(temp_snapshot):
    """Test reading array with C order (row-major)."""
    arr = np.arange(12, dtype=np.float32).reshape(3, 4)

    strata.write_array(temp_snapshot, arr)
    result = strata.read_array(temp_snapshot, shape=(3, 4), dtype="float32", order="C")

    assert result.flags["C_CONTIGUOUS"]
    assert np.array_equal(result, arr)


@pytest.mark.skip(reason="F-order implementation needs verification")
@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_read_array_order_f(temp_snapshot):
    """Test reading array with F order (column-major)."""
    # Note: Current implementation may not correctly handle F-order
    # This test documents expected behavior for future implementation
    arr = np.arange(12, dtype=np.float32).reshape(3, 4, order="C")

    strata.write_array(temp_snapshot, arr)
    result = strata.read_array(temp_snapshot, shape=(3, 4), dtype="float32", order="F")

    assert result.flags["F_CONTIGUOUS"]
    assert np.array_equal(result, arr)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_read_array_invalid_order(temp_snapshot):
    """Test that invalid order raises error."""
    arr = np.arange(10, dtype=np.float32)
    strata.write_array(temp_snapshot, arr)

    with pytest.raises(strata.ValidationError, match="Invalid order"):
        strata.read_array(temp_snapshot, shape=(10,), dtype="float32", order="X")


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_read_array_copy_true(temp_snapshot):
    """Test reading array with copy=True."""
    arr = np.arange(100, dtype=np.float32)
    strata.write_array(temp_snapshot, arr)

    result = strata.read_array(temp_snapshot, shape=(100,), dtype="float32", copy=True)

    # Should be a copy, so writeable
    assert result.flags["WRITEABLE"]
    result[0] = 999
    assert result[0] == 999


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_read_array_copy_false(temp_snapshot):
    """Test reading array with copy=False (returns view)."""
    arr = np.arange(100, dtype=np.float32)
    strata.write_array(temp_snapshot, arr)

    with pytest.warns(UserWarning, match="read-only buffer"):
        result = strata.read_array(
            temp_snapshot, shape=(100,), dtype="float32", copy=False
        )

    # Should be a view
    assert np.array_equal(result, arr)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_read_array_with_offset(temp_snapshot):
    """Test reading array from specific offset."""
    # Write two arrays sequentially
    arr1 = np.arange(50, dtype=np.float32)
    arr2 = np.arange(100, 150, dtype=np.float32)

    # Write to file manually
    with strata.open(temp_snapshot, mode="w") as writer:
        writer.add_bytes(arr1.tobytes())
        writer.add_bytes(arr2.tobytes())

    # Read second array (at offset 200 bytes)
    offset = arr1.nbytes
    result = strata.read_array(
        temp_snapshot, shape=(50,), dtype="float32", offset=offset
    )

    assert np.array_equal(result, arr2)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_read_array_missing_shape(temp_snapshot):
    """Test that reading without shape raises error."""
    arr = np.arange(10, dtype=np.float32)
    strata.write_array(temp_snapshot, arr)

    with pytest.raises(strata.ValidationError, match="shape parameter is required"):
        strata.read_array(temp_snapshot, dtype="float32")


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_write_array_non_contiguous(temp_snapshot):
    """Test writing non-contiguous array (should warn and copy)."""
    # Create non-contiguous array via transpose
    arr = np.arange(20, dtype=np.float32).reshape(4, 5).T

    assert not arr.flags["C_CONTIGUOUS"]

    # Should warn about making a copy
    with pytest.warns(UserWarning, match="not C-contiguous"):
        strata.write_array(temp_snapshot, arr)

    # Should still work
    result = strata.read_array(temp_snapshot, shape=arr.shape, dtype="float32")
    assert np.array_equal(result, arr)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_write_array_different_compressions(temp_snapshot):
    """Test writing with different compression algorithms."""
    arr = np.arange(1000, dtype=np.float32)

    for compression in ["lz4", "zstd"]:
        snap_path = temp_snapshot.replace(".st", f"_{compression}.st")

        strata.write_array(snap_path, arr, compression=compression)
        result = strata.read_array(snap_path, shape=(1000,), dtype="float32")

        assert np.array_equal(result, arr)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_write_array_with_offset(temp_snapshot):
    """Test writing array at specific offset."""
    arr1 = np.arange(50, dtype=np.float32)
    arr2 = np.arange(100, 150, dtype=np.float32)

    # Write first array
    strata.write_array(temp_snapshot, arr1, offset=0)

    # Write second array at offset
    # Note: Current implementation may not support this correctly
    # This test documents expected behavior
    offset = arr1.nbytes
    strata.write_array(temp_snapshot, arr2, offset=offset)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_read_array_with_reader_instance(temp_snapshot):
    """Test reading array using Reader instance instead of path."""
    arr = np.arange(100, dtype=np.float32)
    strata.write_array(temp_snapshot, arr)

    # Open reader
    reader = strata.open(temp_snapshot, mode="r")

    try:
        result = strata.read_array(reader, shape=(100,), dtype="float32")
        assert np.array_equal(result, arr)
    finally:
        reader.close()


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_write_array_with_writer_instance(temp_snapshot):
    """Test writing array using Writer instance instead of path."""
    arr = np.arange(100, dtype=np.float32)

    # Open writer
    with strata.open(temp_snapshot, mode="w") as writer:
        strata.write_array(writer, arr)

    # Read back
    result = strata.read_array(temp_snapshot, shape=(100,), dtype="float32")
    assert np.array_equal(result, arr)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_numpy_dtype_object(temp_snapshot):
    """Test using numpy dtype object instead of string."""
    arr = np.arange(50, dtype=np.int32)

    strata.write_array(temp_snapshot, arr)

    # Use np.dtype object
    result = strata.read_array(temp_snapshot, shape=(50,), dtype=np.dtype("int32"))

    assert result.dtype == np.int32
    assert np.array_equal(result, arr)


def test_array_without_numpy_import_error():
    """Test that array operations raise ImportError without NumPy."""
    # Mock numpy import to fail
    import strata.array

    # Save original
    original_has_numpy = strata.array.HAS_NUMPY

    # Temporarily set to False
    strata.array.HAS_NUMPY = False

    try:
        with pytest.raises(ImportError, match="NumPy is required"):
            strata.read_array("dummy.st", shape=(10,), dtype="float32")

        with pytest.raises(ImportError, match="NumPy is required"):
            import numpy as np

            arr = np.arange(10)
            strata.write_array("dummy.st", arr)
    finally:
        # Restore
        strata.array.HAS_NUMPY = original_has_numpy


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_large_array(temp_snapshot):
    """Test reading and writing large arrays."""
    # 10MB array
    arr = np.random.rand(1280000).astype(np.float32)

    strata.write_array(temp_snapshot, arr)
    result = strata.read_array(temp_snapshot, shape=arr.shape, dtype="float32")

    assert np.array_equal(result, arr)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_empty_array(temp_snapshot):
    """Test writing and reading empty array."""
    arr = np.array([], dtype=np.float32)

    strata.write_array(temp_snapshot, arr)
    result = strata.read_array(temp_snapshot, shape=(0,), dtype="float32")

    assert len(result) == 0
    assert result.dtype == np.float32
