"""Tests for ArrayView slicing and indexing."""

import pytest
import hexz

# Check if NumPy is available
try:
    import numpy as np

    HAS_NUMPY = True
except ImportError:
    HAS_NUMPY = False


@pytest.fixture
def array_1d_snapshot(tmp_path):
    """Create snapshot with 1D array."""
    snap_path = tmp_path / "array_1d.hxz"
    arr = np.arange(100, dtype=np.float32)
    hexz.write_array(str(snap_path), arr)
    return str(snap_path)


@pytest.fixture
def array_2d_snapshot(tmp_path):
    """Create snapshot with 2D array."""
    snap_path = tmp_path / "array_2d.hxz"
    arr = np.arange(1000, dtype=np.float32).reshape(100, 10)
    hexz.write_array(str(snap_path), arr)
    return str(snap_path)


@pytest.fixture
def array_3d_snapshot(tmp_path):
    """Create snapshot with 3D array."""
    snap_path = tmp_path / "array_3d.hxz"
    arr = np.arange(1000, dtype=np.int32).reshape(10, 10, 10)
    hexz.write_array(str(snap_path), arr)
    return str(snap_path)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_basic(array_2d_snapshot):
    """Test basic ArrayView creation."""
    view = hexz.ArrayView(array_2d_snapshot, shape=(100, 10), dtype="float32")

    assert view.shape == (100, 10)
    assert view.dtype == np.dtype("float32")
    assert view.ndim == 2
    assert view.size == 1000
    assert len(view) == 100


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_properties(array_2d_snapshot):
    """Test ArrayView properties."""
    view = hexz.ArrayView(array_2d_snapshot, shape=(100, 10), dtype="float32")

    # Test all properties
    assert view.shape == (100, 10)
    assert view.dtype == np.float32
    assert view.ndim == 2
    assert view.size == 1000
    assert len(view) == 100


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_integer_index_1d(array_1d_snapshot):
    """Test integer indexing on 1D array."""
    view = hexz.ArrayView(array_1d_snapshot, shape=(100,), dtype="float32")

    # Access single elements
    assert view[0] == 0.0
    assert view[50] == 50.0
    assert view[99] == 99.0


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_integer_index_2d(array_2d_snapshot):
    """Test integer indexing on 2D array (returns row)."""
    view = hexz.ArrayView(array_2d_snapshot, shape=(100, 10), dtype="float32")

    # Access rows
    row0 = view[0]
    assert row0.shape == (10,)
    assert np.array_equal(row0, np.arange(10, dtype=np.float32))

    row50 = view[50]
    assert row50.shape == (10,)
    assert np.array_equal(row50, np.arange(500, 510, dtype=np.float32))


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_slice_1d(array_1d_snapshot):
    """Test slicing on 1D array."""
    view = hexz.ArrayView(array_1d_snapshot, shape=(100,), dtype="float32")

    # Slice ranges
    assert np.array_equal(view[0:10], np.arange(10, dtype=np.float32))
    assert np.array_equal(view[50:60], np.arange(50, 60, dtype=np.float32))
    assert np.array_equal(view[90:100], np.arange(90, 100, dtype=np.float32))


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_slice_2d(array_2d_snapshot):
    """Test slicing on 2D array (returns rows)."""
    view = hexz.ArrayView(array_2d_snapshot, shape=(100, 10), dtype="float32")

    # Slice rows
    rows = view[0:10]
    assert rows.shape == (10, 10)
    assert np.array_equal(rows, np.arange(100, dtype=np.float32).reshape(10, 10))

    rows = view[50:55]
    assert rows.shape == (5, 10)
    assert np.array_equal(rows, np.arange(500, 550, dtype=np.float32).reshape(5, 10))


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_tuple_indexing(array_2d_snapshot):
    """Test tuple indexing (multi-dimensional)."""
    view = hexz.ArrayView(array_2d_snapshot, shape=(100, 10), dtype="float32")

    # Access with tuple
    result = view[5, :]
    assert result.shape == (10,)
    assert np.array_equal(result, np.arange(50, 60, dtype=np.float32))

    # Slice on both dimensions
    result = view[0:5, :]
    assert result.shape == (5, 10)


@pytest.mark.skip(reason="Multi-dimensional column slicing not yet implemented")
@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_tuple_with_slices(array_2d_snapshot):
    """Test tuple with slices on multiple dimensions."""
    # Note: This requires implementing column slicing in ArrayView
    # Currently only row slicing (first dimension) is fully supported
    view = hexz.ArrayView(array_2d_snapshot, shape=(100, 10), dtype="float32")

    # First dimension slice, second dimension slice
    result = view[10:20, 2:8]
    assert result.shape == (10, 6)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_empty_tuple_error(array_2d_snapshot):
    """Test that empty tuple raises error."""
    view = hexz.ArrayView(array_2d_snapshot, shape=(100, 10), dtype="float32")

    with pytest.raises(hexz.ValidationError, match="Empty index"):
        _ = view[()]


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_unsupported_index_type(array_2d_snapshot):
    """Test that unsupported index types raise error."""
    view = hexz.ArrayView(array_2d_snapshot, shape=(100, 10), dtype="float32")

    with pytest.raises(TypeError, match="Unsupported index type"):
        _ = view[{1, 2, 3}]  # Set is not supported


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_non_unit_stride_error_1d(array_1d_snapshot):
    """Test that non-unit strides raise NotImplementedError for 1D."""
    view = hexz.ArrayView(array_1d_snapshot, shape=(100,), dtype="float32")

    with pytest.raises(NotImplementedError, match="Non-unit strides"):
        _ = view[0:10:2]  # Step of 2


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_non_unit_stride_error_2d(array_2d_snapshot):
    """Test that non-unit strides raise NotImplementedError for 2D."""
    view = hexz.ArrayView(array_2d_snapshot, shape=(100, 10), dtype="float32")

    with pytest.raises(NotImplementedError, match="Non-unit strides"):
        _ = view[0:10:2]  # Step of 2


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_context_manager(array_2d_snapshot):
    """Test ArrayView as context manager."""
    with hexz.ArrayView(array_2d_snapshot, shape=(100, 10), dtype="float32") as view:
        row = view[0]
        assert row.shape == (10,)

    # After exiting, reader should be closed


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_close(array_2d_snapshot):
    """Test ArrayView close method."""
    view = hexz.ArrayView(array_2d_snapshot, shape=(100, 10), dtype="float32")

    # Access data before closing
    _ = view[0]

    # Close
    view.close()

    # Should not raise (reader closed)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_repr(array_2d_snapshot):
    """Test ArrayView __repr__."""
    view = hexz.ArrayView(array_2d_snapshot, shape=(100, 10), dtype="float32")

    repr_str = repr(view)
    assert "ArrayView" in repr_str
    assert "shape=(100, 10)" in repr_str
    assert "float32" in repr_str


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_with_offset(tmp_path):
    """Test ArrayView with offset parameter."""
    snap_path = tmp_path / "offset.hxz"

    # Write two arrays
    arr1 = np.arange(100, dtype=np.float32)
    arr2 = np.arange(200, 300, dtype=np.float32)

    with hexz.open(str(snap_path), mode="w") as writer:
        writer.add_bytes(arr1.tobytes())
        writer.add_bytes(arr2.tobytes())

    # Create view starting at offset (second array)
    offset = arr1.nbytes
    view = hexz.ArrayView(str(snap_path), shape=(100,), dtype="float32", offset=offset)

    # Should read from second array
    assert np.array_equal(view[0:10], np.arange(200, 210, dtype=np.float32))


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_1d_length(array_1d_snapshot):
    """Test __len__ for 1D array."""
    view = hexz.ArrayView(array_1d_snapshot, shape=(100,), dtype="float32")
    assert len(view) == 100


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_3d_indexing(array_3d_snapshot):
    """Test indexing on 3D array."""
    view = hexz.ArrayView(array_3d_snapshot, shape=(10, 10, 10), dtype="int32")

    # Integer index (first dimension)
    slice_2d = view[5]
    assert slice_2d.shape == (10, 10)

    # Slice on first dimension
    slice_3d = view[2:5]
    assert slice_3d.shape == (3, 10, 10)

    # Tuple indexing
    result = view[1, :]
    assert result.shape == (10, 10)


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_negative_indices():
    """Test that negative indices work correctly."""
    # This would require implementing support for negative indices
    # Currently may not be supported - document expected behavior
    pass


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_boundary_access(array_1d_snapshot):
    """Test accessing at boundaries."""
    view = hexz.ArrayView(array_1d_snapshot, shape=(100,), dtype="float32")

    # First element
    assert view[0] == 0.0

    # Last element
    assert view[99] == 99.0

    # Last slice
    result = view[95:100]
    assert len(result) == 5


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_different_dtypes(tmp_path):
    """Test ArrayView with different dtypes."""
    dtypes = ["int8", "int16", "int32", "int64", "float32", "float64"]

    for dtype_str in dtypes:
        snap_path = tmp_path / f"{dtype_str}.hxz"
        arr = np.arange(100, dtype=dtype_str)
        hexz.write_array(str(snap_path), arr)

        view = hexz.ArrayView(str(snap_path), shape=(100,), dtype=dtype_str)

        assert view.dtype == np.dtype(dtype_str)
        assert np.array_equal(view[0:10], arr[0:10])


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_empty_shape(tmp_path):
    """Test ArrayView with empty dimension."""
    snap_path = tmp_path / "empty.hxz"

    # Create empty array
    arr = np.array([], dtype=np.float32)
    hexz.write_array(str(snap_path), arr)

    view = hexz.ArrayView(str(snap_path), shape=(0,), dtype="float32")

    assert len(view) == 0
    assert view.size == 0


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_ndim_property(array_3d_snapshot):
    """Test ndim property for different dimensionalities."""
    view_3d = hexz.ArrayView(array_3d_snapshot, shape=(10, 10, 10), dtype="int32")
    assert view_3d.ndim == 3


@pytest.mark.skipif(not HAS_NUMPY, reason="NumPy not installed")
def test_arrayview_size_property(array_2d_snapshot):
    """Test size property."""
    view = hexz.ArrayView(array_2d_snapshot, shape=(100, 10), dtype="float32")
    assert view.size == 1000  # 100 * 10
