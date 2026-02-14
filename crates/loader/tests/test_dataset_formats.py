"""Tests for Dataset output format handling."""

import pytest
import hexz

# Check if optional dependencies are available
try:
    import torch

    HAS_TORCH = True
except ImportError:
    HAS_TORCH = False

try:
    import numpy as np

    HAS_NUMPY = True
except ImportError:
    HAS_NUMPY = False


@pytest.fixture
def test_snapshot(tmp_path):
    """Create a test snapshot with known data."""
    snap_path = tmp_path / "test.hxz"
    data_file = tmp_path / "data.bin"

    # Create 10 items of 100 bytes each with known pattern
    with open(data_file, "wb") as f:
        for i in range(10):
            # Each item: first byte is index, rest are zeros
            item = bytes([i]) + bytes(99)
            f.write(item)

    # Pack it
    with hexz.open(str(snap_path), mode="w") as writer:
        writer.add(str(data_file))

    return str(snap_path)


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_output_format_tensor(test_snapshot):
    """Test tensor output format."""
    dataset = hexz.Dataset(test_snapshot, item_size=100, output_format="tensor")

    item = dataset[3]

    assert isinstance(item, torch.Tensor)
    assert item.dtype == torch.uint8
    assert len(item) == 100
    assert item[0].item() == 3  # First byte should be index
    assert item[1].item() == 0  # Rest should be zeros


@pytest.mark.skipif(not HAS_TORCH or not HAS_NUMPY, reason="Libraries not installed")
def test_output_format_numpy(test_snapshot):
    """Test numpy output format."""
    dataset = hexz.Dataset(test_snapshot, item_size=100, output_format="numpy")

    item = dataset[5]

    assert isinstance(item, np.ndarray)
    assert item.dtype == np.uint8
    assert len(item) == 100
    assert item[0] == 5  # First byte should be index
    assert item[1] == 0  # Rest should be zeros


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_output_format_bytes(test_snapshot):
    """Test bytes output format."""
    dataset = hexz.Dataset(test_snapshot, item_size=100, output_format="bytes")

    item = dataset[7]

    assert isinstance(item, bytes)
    assert len(item) == 100
    assert item[0] == 7  # First byte should be index
    assert item[1] == 0  # Rest should be zeros


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_invalid_output_format(test_snapshot):
    """Test that invalid output format raises error."""
    # Create dataset with valid format first
    dataset = hexz.Dataset(test_snapshot, item_size=100, output_format="bytes")

    # Manually change to invalid format to test _decode_item error path
    dataset._output_format = "invalid"

    with pytest.raises(ValueError, match="Invalid output_format"):
        _ = dataset[0]


@pytest.mark.skipif(not HAS_TORCH or not HAS_NUMPY, reason="Libraries not installed")
def test_numpy_zero_copy_true(test_snapshot):
    """Test numpy format with zero_copy=True (returns view)."""
    dataset = hexz.Dataset(
        test_snapshot, item_size=100, output_format="numpy", zero_copy=True
    )

    item = dataset[0]

    assert isinstance(item, np.ndarray)
    # With zero_copy=True, should be a view (no copy made)
    # The array should still have the correct data
    assert len(item) == 100


@pytest.mark.skipif(not HAS_TORCH or not HAS_NUMPY, reason="Libraries not installed")
def test_numpy_zero_copy_false(test_snapshot):
    """Test numpy format with zero_copy=False (makes copy)."""
    dataset = hexz.Dataset(
        test_snapshot, item_size=100, output_format="numpy", zero_copy=False
    )

    item = dataset[0]

    assert isinstance(item, np.ndarray)
    # Should have made a copy
    assert len(item) == 100
    # Data should be writable
    item[0] = 255  # Should not raise


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_tensor_format_from_bytearray(test_snapshot):
    """Test that tensor format uses bytearray correctly."""
    dataset = hexz.Dataset(test_snapshot, item_size=100, output_format="tensor")

    item = dataset[2]

    # torch.frombuffer with bytearray should work
    assert isinstance(item, torch.Tensor)
    assert item[0].item() == 2


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_format_with_transform(test_snapshot):
    """Test that transform is applied after format conversion."""

    def double_transform(x):
        return x * 2

    # Test with tensor format
    dataset = hexz.Dataset(
        test_snapshot,
        item_size=100,
        output_format="tensor",
        transform=double_transform,
    )

    item = dataset[1]
    # Item 1 has first byte = 1, after transform should be 2
    assert item[0].item() == 2
    assert item[1].item() == 0  # Was 0, still 0


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_format_with_target_transform(test_snapshot):
    """Test target_transform parameter (stored but not used in basic Dataset)."""

    def target_transform(x):
        return x + 1

    # Create dataset with target_transform
    dataset = hexz.Dataset(
        test_snapshot,
        item_size=100,
        output_format="bytes",
        target_transform=target_transform,
    )

    # Basic dataset doesn't return targets, just items
    # So target_transform is stored but not applied
    item = dataset[0]
    assert isinstance(item, bytes)


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_bytes_format_no_transform(test_snapshot):
    """Test bytes format returns raw bytes without modification."""
    dataset = hexz.Dataset(test_snapshot, item_size=100, output_format="bytes")

    item = dataset[9]

    # Should be raw bytes
    assert isinstance(item, bytes)
    assert len(item) == 100
    assert item[0] == 9


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_format_consistency_across_access(test_snapshot):
    """Test that format is consistent across multiple accesses."""
    dataset = hexz.Dataset(test_snapshot, item_size=100, output_format="tensor")

    item1 = dataset[4]
    item2 = dataset[4]

    # Should be same type and content
    assert type(item1) is type(item2)
    assert torch.equal(item1, item2)
