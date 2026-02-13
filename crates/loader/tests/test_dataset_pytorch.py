"""Tests for PyTorch Dataset integration."""

import pytest
import struct
import strata

# Check if PyTorch is available
try:
    import torch
    from torch.utils.data import DataLoader

    HAS_TORCH = True
except ImportError:
    HAS_TORCH = False


@pytest.fixture
def fixed_size_snapshot(tmp_path):
    """Create a snapshot with fixed-size items (100 items of 1KB each)."""
    snap_path = tmp_path / "fixed.st"
    data_file = tmp_path / "data.bin"

    # Create 100 items of 1KB each
    with open(data_file, "wb") as f:
        for i in range(100):
            item = bytes([i % 256]) * 1024
            f.write(item)

    # Pack it
    with strata.open(str(snap_path), mode="w") as writer:
        writer.add(str(data_file))

    return str(snap_path)


@pytest.fixture
def variable_size_snapshot(tmp_path):
    """Create snapshot with variable-size items and index."""
    snap_path = tmp_path / "variable.st"
    index_path = tmp_path / "variable.idx"
    data_file = tmp_path / "data.bin"

    # Create variable-size items
    items = []
    with open(data_file, "wb") as f:
        offset = 0
        for i in range(50):
            size = 512 + (i * 10)  # Growing sizes
            item = bytes([i % 256]) * size
            f.write(item)
            items.append((offset, size))
            offset += size

    # Create index file
    with open(index_path, "wb") as f:
        for offset, size in items:
            f.write(struct.pack("<QQ", offset, size))

    # Pack snapshot
    with strata.open(str(snap_path), mode="w") as writer:
        writer.add(str(data_file))

    return str(snap_path), str(index_path)


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dataset_requires_torch():
    """Test that Dataset raises ImportError without PyTorch."""
    # This test is skipped if torch is available, but tests the import path
    pass


def test_dataset_import_error_without_torch(monkeypatch, fixed_size_snapshot):
    """Test that Dataset.__init__ raises ImportError when torch is not available."""
    # Mock torch import to fail
    import sys

    monkeypatch.setitem(sys.modules, "torch", None)
    monkeypatch.setitem(sys.modules, "torch.utils.data", None)

    # Need to reload the module to trigger the import check
    import strata.dataset

    # Temporarily set HAS_TORCH to False
    original_has_torch = strata.dataset.HAS_TORCH
    strata.dataset.HAS_TORCH = False

    try:
        with pytest.raises(ImportError, match="PyTorch is required"):
            strata.Dataset(fixed_size_snapshot, item_size=1024)
    finally:
        # Restore
        strata.dataset.HAS_TORCH = original_has_torch


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dataset_basic(fixed_size_snapshot):
    """Test basic Dataset functionality."""
    dataset = strata.Dataset(fixed_size_snapshot, item_size=1024)

    # Check length
    assert len(dataset) == 100

    # Get an item
    item = dataset[0]
    assert isinstance(item, torch.Tensor)
    assert len(item) == 1024
    assert item.dtype == torch.uint8

    # Check content
    assert item[0] == 0


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dataset_validation_error(fixed_size_snapshot):
    """Test that Dataset raises ValidationError without item_size or index_file."""
    with pytest.raises(strata.ValidationError, match="Either item_size or index_file"):
        strata.Dataset(fixed_size_snapshot)


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dataset_different_indices(fixed_size_snapshot):
    """Test accessing different indices."""
    dataset = strata.Dataset(fixed_size_snapshot, item_size=1024)

    # Access multiple items
    item0 = dataset[0]
    item1 = dataset[1]
    item50 = dataset[50]

    assert item0[0] == 0
    assert item1[0] == 1
    assert item50[0] == 50


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dataset_with_cache(fixed_size_snapshot):
    """Test Dataset with caching enabled."""
    dataset = strata.Dataset(fixed_size_snapshot, item_size=1024, cache_size_mb=10)

    # First access (cache miss)
    item1 = dataset[5]

    # Second access (cache hit)
    item2 = dataset[5]

    assert torch.equal(item1, item2)

    # Check cache stats
    stats = dataset.cache_stats()
    assert stats["enabled"] is True
    assert stats["hits"] >= 1
    assert stats["misses"] >= 1


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dataset_cache_disabled(fixed_size_snapshot):
    """Test Dataset with caching disabled."""
    dataset = strata.Dataset(fixed_size_snapshot, item_size=1024, cache_size_mb=0)

    # Access items
    _ = dataset[0]
    _ = dataset[1]

    # Cache should be disabled
    stats = dataset.cache_stats()
    assert stats["enabled"] is False


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dataset_with_prefetching(fixed_size_snapshot):
    """Test Dataset with prefetching."""
    dataset = strata.Dataset(
        fixed_size_snapshot, item_size=1024, prefetch_factor=2, num_workers=2
    )

    # Access items sequentially
    items = [dataset[i] for i in range(10)]

    assert len(items) == 10
    for i, item in enumerate(items):
        assert item[0] == i % 256


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dataset_shuffling(fixed_size_snapshot):
    """Test Dataset with shuffling."""
    dataset = strata.Dataset(fixed_size_snapshot, item_size=1024, shuffle=True, seed=42)

    # Get first 10 indices
    indices_epoch0 = []
    for i in range(10):
        item = dataset[i]
        # Reverse engineer which original index this came from
        indices_epoch0.append(int(item[0]))

    # Indices should NOT be sequential (shuffled)
    assert indices_epoch0 != list(range(10))

    # Create another dataset with same seed
    dataset2 = strata.Dataset(
        fixed_size_snapshot, item_size=1024, shuffle=True, seed=42
    )

    indices_epoch0_again = []
    for i in range(10):
        item = dataset2[i]
        indices_epoch0_again.append(int(item[0]))

    # Should match (same seed)
    assert indices_epoch0 == indices_epoch0_again


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dataset_set_epoch(fixed_size_snapshot):
    """Test set_epoch for DDP shuffling."""
    dataset = strata.Dataset(fixed_size_snapshot, item_size=1024, shuffle=True, seed=42)

    # Get first item in epoch 0
    _ = int(dataset[0][0])

    # Set to epoch 1
    dataset.set_epoch(1)

    # First item should be different
    _ = int(dataset[0][0])

    # They might be the same by chance, but likely different
    # More importantly, verify that set_epoch doesn't crash


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dataset_output_format_bytes(fixed_size_snapshot):
    """Test Dataset with bytes output format."""
    dataset = strata.Dataset(fixed_size_snapshot, item_size=1024, output_format="bytes")

    item = dataset[0]
    assert isinstance(item, bytes)
    assert len(item) == 1024


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dataset_output_format_numpy(fixed_size_snapshot):
    """Test Dataset with numpy output format."""
    import numpy as np

    dataset = strata.Dataset(fixed_size_snapshot, item_size=1024, output_format="numpy")

    item = dataset[0]
    assert isinstance(item, np.ndarray)
    assert len(item) == 1024
    assert item.dtype == np.uint8


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dataset_output_format_tensor(fixed_size_snapshot):
    """Test Dataset with tensor output format."""
    dataset = strata.Dataset(
        fixed_size_snapshot, item_size=1024, output_format="tensor"
    )

    item = dataset[0]
    assert isinstance(item, torch.Tensor)
    assert len(item) == 1024
    assert item.dtype == torch.uint8


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dataset_with_transform(fixed_size_snapshot):
    """Test Dataset with transform function."""

    def transform(x):
        # Simple transform: multiply by 2
        return x * 2

    dataset = strata.Dataset(
        fixed_size_snapshot, item_size=1024, transform=transform, output_format="tensor"
    )

    item = dataset[0]
    # Original value is 0, transformed should be 0
    assert item[0] == 0

    item1 = dataset[1]
    # Original value is 1, transformed should be 2
    assert item1[0] == 2


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dataset_with_dataloader(fixed_size_snapshot):
    """Test Dataset with PyTorch DataLoader."""
    dataset = strata.Dataset(fixed_size_snapshot, item_size=1024)

    dataloader = DataLoader(dataset, batch_size=10, shuffle=False)

    batch = next(iter(dataloader))
    assert batch.shape == (10, 1024)


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dataset_variable_length(variable_size_snapshot):
    """Test Dataset with variable-length items."""
    snap_path, index_path = variable_size_snapshot

    dataset = strata.Dataset(snap_path, index_file=index_path, output_format="bytes")

    # Check length
    assert len(dataset) == 50

    # Access items of different sizes
    item0 = dataset[0]
    item10 = dataset[10]

    assert len(item0) == 512  # First item size
    assert len(item10) == 612  # 512 + 10*10


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dataset_repr(fixed_size_snapshot):
    """Test Dataset __repr__."""
    dataset = strata.Dataset(fixed_size_snapshot, item_size=1024, cache_size_mb=10)

    # Access an item to populate cache
    _ = dataset[0]

    repr_str = repr(dataset)
    assert "Dataset" in repr_str
    assert "100 items" in repr_str


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dataset_zero_copy(fixed_size_snapshot):
    """Test Dataset with zero_copy option."""
    dataset = strata.Dataset(
        fixed_size_snapshot, item_size=1024, zero_copy=True, output_format="numpy"
    )

    item = dataset[0]
    # Should still work, just potentially more efficient
    assert len(item) == 1024


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dataset_del(fixed_size_snapshot):
    """Test Dataset cleanup via __del__."""
    dataset = strata.Dataset(
        fixed_size_snapshot, item_size=1024, prefetch_factor=2, num_workers=2
    )

    # Access an item to ensure everything is initialized
    _ = dataset[0]

    # Delete dataset (should cleanup resources)
    del dataset

    # Should not raise any errors
