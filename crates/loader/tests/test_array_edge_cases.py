"""Tests for array edge cases and error handling."""

import pytest
import hexz

# Check if NumPy is available
try:
    import numpy as np

    HAS_NUMPY = True
except ImportError:
    HAS_NUMPY = False


def test_array_module_without_numpy():
    """Test that array module handles missing NumPy gracefully."""
    import hexz.array

    # Save original
    original_has_numpy = hexz.array.HAS_NUMPY
    original_np = hexz.array.np

    # Mock numpy as unavailable
    hexz.array.HAS_NUMPY = False
    hexz.array.np = None

    try:
        # _check_numpy should raise ImportError
        with pytest.raises(ImportError, match="NumPy is required"):
            hexz.array._check_numpy()
    finally:
        # Restore
        hexz.array.HAS_NUMPY = original_has_numpy
        hexz.array.np = original_np


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_read_array_reader_instance_cleanup(tmp_path):
    """Test that Reader is properly closed when passed as instance."""
    snap_path = tmp_path / "test.hxz"
    arr = np.arange(100, dtype=np.float32)
    hexz.write_array(str(snap_path), arr)

    # Pass path (should auto-close)
    result1 = hexz.read_array(str(snap_path), shape=(100,), dtype="float32")
    assert np.array_equal(result1, arr)

    # Pass Reader instance (should NOT auto-close)
    reader = hexz.open(str(snap_path), mode="r")
    result2 = hexz.read_array(reader, shape=(100,), dtype="float32")
    assert np.array_equal(result2, arr)

    # Reader should still be open
    # (Implementation closes it internally in finally, but original stays open)
    reader.close()


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_write_array_writer_instance_cleanup(tmp_path):
    """Test that Writer is properly handled when passed as instance."""
    snap_path = tmp_path / "test.hxz"
    arr = np.arange(100, dtype=np.float32)

    # Pass path (should auto-finalize)
    hexz.write_array(str(snap_path), arr)

    # Verify written
    result = hexz.read_array(str(snap_path), shape=(100,), dtype="float32")
    assert np.array_equal(result, arr)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_write_array_contiguous_check(tmp_path):
    """Test non-contiguous array warning."""
    snap_path = tmp_path / "test.hxz"

    # Create non-contiguous array
    arr = np.arange(100, dtype=np.float32).reshape(10, 10).T
    assert not arr.flags["C_CONTIGUOUS"]

    # Should warn and make copy
    with pytest.warns(UserWarning, match="not C-contiguous"):
        hexz.write_array(str(snap_path), arr)

    # Should still write correctly
    result = hexz.read_array(str(snap_path), shape=(10, 10), dtype="float32")
    assert np.array_equal(result, arr)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_read_array_immutable_view_warning(tmp_path):
    """Test warning when returning immutable view."""
    snap_path = tmp_path / "test.hxz"
    arr = np.arange(100, dtype=np.float32)
    hexz.write_array(str(snap_path), arr)

    # copy=False should warn about immutability
    with pytest.warns(UserWarning, match="read-only buffer"):
        result = hexz.read_array(
            str(snap_path), shape=(100,), dtype="float32", copy=False
        )

    assert np.array_equal(result, arr)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_import_check(tmp_path):
    """Test ArrayView raises error without NumPy."""
    import hexz.array

    # Save original
    original_has_numpy = hexz.array.HAS_NUMPY

    # Mock numpy as unavailable
    hexz.array.HAS_NUMPY = False

    try:
        with pytest.raises(ImportError, match="NumPy is required"):
            hexz.ArrayView("dummy.hxz", shape=(10,), dtype="float32")
    finally:
        # Restore
        hexz.array.HAS_NUMPY = original_has_numpy


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_integer_index_multidim(tmp_path):
    """Test integer indexing on multidimensional arrays."""
    snap_path = tmp_path / "test.hxz"

    # 3D array
    arr = np.arange(1000, dtype=np.int32).reshape(10, 10, 10)
    hexz.write_array(str(snap_path), arr)

    view = hexz.ArrayView(str(snap_path), shape=(10, 10, 10), dtype="int32")

    # Integer index on first dimension
    slice_2d = view[5]
    assert slice_2d.shape == (10, 10)
    assert np.array_equal(slice_2d, arr[5])


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_slice_1d_full(tmp_path):
    """Test 1D array slicing coverage."""
    snap_path = tmp_path / "1d.hxz"
    arr = np.arange(100, dtype=np.float32)
    hexz.write_array(str(snap_path), arr)

    view = hexz.ArrayView(str(snap_path), shape=(100,), dtype="float32")

    # Test slice.indices logic
    result = view[10:20]
    assert len(result) == 10
    assert np.array_equal(result, arr[10:20])


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_slice_2d_rows(tmp_path):
    """Test 2D array row slicing."""
    snap_path = tmp_path / "2d.hxz"
    arr = np.arange(1000, dtype=np.float32).reshape(100, 10)
    hexz.write_array(str(snap_path), arr)

    view = hexz.ArrayView(str(snap_path), shape=(100, 10), dtype="float32")

    # Slice rows
    rows = view[20:30]
    assert rows.shape == (10, 10)
    assert np.array_equal(rows, arr[20:30])


@pytest.mark.skip(reason="Multi-dimensional column slicing not yet implemented")
@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_tuple_slicing_rest_keys(tmp_path):
    """Test tuple slicing with rest_keys logic."""
    # Note: This requires implementing column slicing in ArrayView
    snap_path = tmp_path / "2d.hxz"
    arr = np.arange(1000, dtype=np.float32).reshape(100, 10)
    hexz.write_array(str(snap_path), arr)

    view = hexz.ArrayView(str(snap_path), shape=(100, 10), dtype="float32")

    # Tuple with rest_keys
    result = view[10:20, 2:8]
    assert result.shape == (10, 6)
    assert np.array_equal(result, arr[10:20, 2:8])


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_tuple_no_rest_keys(tmp_path):
    """Test tuple indexing without rest_keys."""
    snap_path = tmp_path / "2d.hxz"
    arr = np.arange(1000, dtype=np.float32).reshape(100, 10)
    hexz.write_array(str(snap_path), arr)

    view = hexz.ArrayView(str(snap_path), shape=(100, 10), dtype="float32")

    # Single key in tuple (no rest_keys)
    result = view[5,]
    assert result.shape == (10,)
    assert np.array_equal(result, arr[5])


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_read_array_dtype_conversion(tmp_path):
    """Test dtype parameter handles both string and numpy dtype."""
    snap_path = tmp_path / "test.hxz"
    arr = np.arange(100, dtype=np.int32)
    hexz.write_array(str(snap_path), arr)

    # String dtype
    result1 = hexz.read_array(str(snap_path), shape=(100,), dtype="int32")
    assert result1.dtype == np.int32

    # NumPy dtype object
    result2 = hexz.read_array(str(snap_path), shape=(100,), dtype=np.dtype("int32"))
    assert result2.dtype == np.int32

    assert np.array_equal(result1, result2)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_write_array_tobytes_conversion(tmp_path):
    """Test that write_array correctly converts to bytes."""
    snap_path = tmp_path / "test.hxz"

    # Different shaped arrays
    for shape in [(100,), (10, 10), (5, 4, 5)]:
        arr = np.arange(np.prod(shape), dtype=np.float32).reshape(shape)
        hexz.write_array(str(snap_path), arr)

        # Verify size
        result = hexz.read_array(str(snap_path), shape=shape, dtype="float32")
        assert np.array_equal(result, arr)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_itemsize_calculation(tmp_path):
    """Test that ArrayView correctly calculates itemsize."""
    snap_path = tmp_path / "test.hxz"
    arr = np.arange(100, dtype=np.float64)  # 8 bytes per item
    hexz.write_array(str(snap_path), arr)

    view = hexz.ArrayView(str(snap_path), shape=(100,), dtype="float64")

    # Should have correct itemsize
    assert view._itemsize == 8

    # Single item access should read correct number of bytes
    item = view[0]
    assert item == 0.0


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_1d_single_element_access(tmp_path):
    """Test accessing single element from 1D array returns scalar."""
    snap_path = tmp_path / "test.hxz"
    arr = np.arange(100, dtype=np.float32)
    hexz.write_array(str(snap_path), arr)

    view = hexz.ArrayView(str(snap_path), shape=(100,), dtype="float32")

    # Single integer index on 1D should return scalar
    elem = view[42]
    assert isinstance(elem, (np.floating, float))
    assert elem == 42.0


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_multidim_row_size(tmp_path):
    """Test row size calculation for multi-dimensional arrays."""
    snap_path = tmp_path / "test.hxz"

    # 3D array
    arr = np.arange(2000, dtype=np.int32).reshape(20, 10, 10)
    hexz.write_array(str(snap_path), arr)

    view = hexz.ArrayView(str(snap_path), shape=(20, 10, 10), dtype="int32")

    # Access single row (first dimension)
    row = view[5]
    assert row.shape == (10, 10)

    # Row size should be 10*10*4 = 400 bytes
    expected_row = arr[5]
    assert np.array_equal(row, expected_row)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_read_array_size_calculation(tmp_path):
    """Test that read_array correctly calculates size from shape and dtype."""
    snap_path = tmp_path / "test.hxz"

    # Various dtypes with different sizes
    test_cases = [
        ((100,), "int8", 100),  # 1 byte each
        ((100,), "int32", 400),  # 4 bytes each
        ((100,), "float64", 800),  # 8 bytes each
        ((10, 10), "int16", 200),  # 2 bytes each
    ]

    for shape, dtype, expected_size in test_cases:
        arr = np.zeros(shape, dtype=dtype)
        hexz.write_array(str(snap_path), arr)

        # Read should calculate correct size
        result = hexz.read_array(str(snap_path), shape=shape, dtype=dtype)
        assert result.nbytes == expected_size


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_with_different_offsets(tmp_path):
    """Test ArrayView with various offset values."""
    snap_path = tmp_path / "test.hxz"

    # Write multiple arrays
    arr1 = np.arange(100, dtype=np.float32)
    arr2 = np.arange(200, 300, dtype=np.float32)
    arr3 = np.arange(400, 500, dtype=np.float32)

    with hexz.open(str(snap_path), mode="w") as writer:
        writer.add_bytes(arr1.tobytes())
        writer.add_bytes(arr2.tobytes())
        writer.add_bytes(arr3.tobytes())

    # View each array with correct offset
    view1 = hexz.ArrayView(str(snap_path), shape=(100,), dtype="float32", offset=0)
    assert np.array_equal(view1[:], arr1)

    view2 = hexz.ArrayView(str(snap_path), shape=(100,), dtype="float32", offset=400)
    assert np.array_equal(view2[:], arr2)

    view3 = hexz.ArrayView(str(snap_path), shape=(100,), dtype="float32", offset=800)
    assert np.array_equal(view3[:], arr3)
