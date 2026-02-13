"""Tests for Dataset Prefetcher functionality."""

import pytest
import time
import strata
from strata.dataset import Prefetcher


@pytest.fixture
def test_snapshot(tmp_path):
    """Create a test snapshot with sequential data."""
    snap_path = tmp_path / "test.st"

    # Create a file with sequential 1KB blocks
    data_file = tmp_path / "data.bin"
    with open(data_file, "wb") as f:
        for i in range(100):
            # Each block is 1KB with a unique pattern
            block = bytes([i % 256]) * 1024
            f.write(block)

    # Pack it
    with strata.open(str(snap_path), mode="w") as writer:
        writer.add(str(data_file))

    return str(snap_path)


def test_prefetcher_basic(test_snapshot):
    """Test basic prefetcher initialization and shutdown."""
    reader = strata.open(test_snapshot)

    prefetcher = Prefetcher(
        reader=reader,
        prefetch_factor=2,
        num_workers=2,
        item_size=1024,
    )

    assert prefetcher.active is True
    assert prefetcher.prefetch_factor == 2
    assert prefetcher.num_workers == 2

    prefetcher.shutdown()
    assert prefetcher.active is False
    assert len(prefetcher.pending) == 0
    assert len(prefetcher.prefetched) == 0


def test_prefetcher_hint_and_get(test_snapshot):
    """Test hinting and retrieving prefetched items."""
    reader = strata.open(test_snapshot)

    prefetcher = Prefetcher(
        reader=reader,
        prefetch_factor=2,
        num_workers=2,
        item_size=1024,
    )

    # Hint that we'll access index 5
    prefetcher.hint(5)

    # Give it a moment to prefetch
    time.sleep(0.1)

    # Should be able to get it
    data = prefetcher.get(5)
    assert data is not None
    assert len(data) == 1024
    assert data == bytes([5 % 256]) * 1024

    prefetcher.shutdown()


def test_prefetcher_duplicate_hint(test_snapshot):
    """Test that duplicate hints don't create duplicate jobs."""
    reader = strata.open(test_snapshot)

    prefetcher = Prefetcher(
        reader=reader,
        prefetch_factor=2,
        num_workers=1,
        item_size=1024,
    )

    # Hint same index multiple times
    prefetcher.hint(10)
    prefetcher.hint(10)
    prefetcher.hint(10)

    # Give it time to process
    time.sleep(0.1)

    # Should only have one entry
    data = prefetcher.get(10)
    assert data is not None

    # Second get should return None (already consumed)
    data2 = prefetcher.get(10)
    assert data2 is None

    prefetcher.shutdown()


def test_prefetcher_miss(test_snapshot):
    """Test getting an item that wasn't prefetched."""
    reader = strata.open(test_snapshot)

    prefetcher = Prefetcher(
        reader=reader,
        prefetch_factor=2,
        num_workers=2,
        item_size=1024,
    )

    # Try to get without hinting
    data = prefetcher.get(100)
    assert data is None

    prefetcher.shutdown()


def test_prefetcher_with_index(test_snapshot, tmp_path):
    """Test prefetcher with variable-length items using an index."""
    import struct

    # Create an index file
    index_file = tmp_path / "test.idx"
    with open(index_file, "wb") as f:
        # First 10 items at different offsets/sizes
        for i in range(10):
            offset = i * 1024
            size = 1024
            f.write(struct.pack("<QQ", offset, size))

    # Load index
    index = []
    with open(index_file, "rb") as f:
        while True:
            chunk = f.read(16)  # 2 * uint64
            if len(chunk) < 16:
                break
            offset, size = struct.unpack("<QQ", chunk)
            index.append((offset, size))

    reader = strata.open(test_snapshot)

    prefetcher = Prefetcher(
        reader=reader,
        prefetch_factor=2,
        num_workers=2,
        index=index,
    )

    # Hint item 3
    prefetcher.hint(3)
    time.sleep(0.1)

    # Should get it
    data = prefetcher.get(3)
    assert data is not None
    assert len(data) == 1024

    prefetcher.shutdown()


def test_prefetcher_inactive_after_shutdown(test_snapshot):
    """Test that hints are ignored after shutdown."""
    reader = strata.open(test_snapshot)

    prefetcher = Prefetcher(
        reader=reader,
        prefetch_factor=2,
        num_workers=2,
        item_size=1024,
    )

    prefetcher.shutdown()

    # Hint after shutdown should be ignored
    prefetcher.hint(5)

    # Should not be in prefetched or pending
    assert len(prefetcher.prefetched) == 0
    assert len(prefetcher.pending) == 0


def test_prefetcher_concurrent_access(test_snapshot):
    """Test concurrent access to prefetcher."""
    reader = strata.open(test_snapshot)

    prefetcher = Prefetcher(
        reader=reader,
        prefetch_factor=5,
        num_workers=4,
        item_size=1024,
    )

    # Hint multiple items
    for i in range(10):
        prefetcher.hint(i)

    # Give workers time to prefetch
    time.sleep(0.2)

    # Get them all
    retrieved = 0
    for i in range(10):
        data = prefetcher.get(i)
        if data is not None:
            retrieved += 1

    # Should have retrieved most of them
    assert retrieved >= 5

    prefetcher.shutdown()


def test_prefetcher_no_item_size_or_index_error(test_snapshot):
    """Test that error is raised if neither item_size nor index is provided."""
    reader = strata.open(test_snapshot)

    prefetcher = Prefetcher(
        reader=reader,
        prefetch_factor=2,
        num_workers=2,
        item_size=None,
        index=None,
    )

    # Hint an item - should fail in worker
    prefetcher.hint(0)
    time.sleep(0.1)

    # Get should return None (prefetch failed)
    data = prefetcher.get(0)
    assert data is None

    prefetcher.shutdown()


def test_prefetcher_pending_future_done(test_snapshot):
    """Test getting an item while its future is still pending."""
    reader = strata.open(test_snapshot)

    prefetcher = Prefetcher(
        reader=reader,
        prefetch_factor=2,
        num_workers=1,  # Single worker to control timing
        item_size=1024,
    )

    # Hint item
    prefetcher.hint(5)

    # Immediately try to get it (might still be pending)
    # Give a tiny moment for the job to be submitted
    time.sleep(0.01)

    # Try to get - should either be prefetched or pending
    data = prefetcher.get(5)

    # Wait a bit more if needed
    if data is None:
        time.sleep(0.15)
        # Try again - should be in prefetched now
        prefetcher.hint(5)
        time.sleep(0.1)
        # It should be available now or hint was ignored (already exists)

    prefetcher.shutdown()


def test_prefetcher_multiple_workers(test_snapshot):
    """Test prefetcher with multiple worker threads."""
    reader = strata.open(test_snapshot)

    # Use 4 workers to prefetch in parallel
    prefetcher = Prefetcher(
        reader=reader,
        prefetch_factor=10,
        num_workers=4,
        item_size=1024,
    )

    # Hint many items
    for i in range(20):
        prefetcher.hint(i)

    # Give workers time
    time.sleep(0.3)

    # Check that multiple items were prefetched
    count = 0
    for i in range(20):
        if prefetcher.get(i) is not None:
            count += 1

    assert count >= 10  # At least half should be prefetched

    prefetcher.shutdown()
