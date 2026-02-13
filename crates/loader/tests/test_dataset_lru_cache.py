"""Tests for Dataset LRUCache functionality."""

from strata.dataset import LRUCache


def test_lru_cache_basic():
    """Test basic cache get/put operations."""
    cache = LRUCache(max_size_mb=1)

    # Initially empty
    assert cache.get(0) is None
    assert cache.misses == 1
    assert cache.hits == 0

    # Add item
    data = b"hello world"
    cache.put(0, data)

    # Retrieve it
    retrieved = cache.get(0)
    assert retrieved == data
    assert cache.hits == 1
    assert cache.misses == 1


def test_lru_cache_eviction():
    """Test LRU eviction when size limit is reached."""
    # 1KB cache
    cache = LRUCache(max_size_mb=0.001)  # ~1KB

    # Add items that exceed cache size
    for i in range(10):
        # Each item is ~200 bytes
        data = b"x" * 200
        cache.put(i, data)

    # Early items should be evicted
    assert cache.get(0) is None  # Evicted
    assert cache.get(9) is not None  # Recent, still in cache

    # Cache should be under size limit
    assert cache.current_size <= cache.max_size_bytes


def test_lru_cache_move_to_end():
    """Test that accessing items moves them to end (most recently used)."""
    cache = LRUCache(max_size_mb=0.001)  # ~1KB

    # Add 3 items
    cache.put(0, b"a" * 300)
    cache.put(1, b"b" * 300)
    cache.put(2, b"c" * 300)

    # Access item 0 to make it recently used
    cache.get(0)

    # Add more items to trigger eviction
    cache.put(3, b"d" * 300)
    cache.put(4, b"e" * 300)

    # Item 0 should still be present (was moved to end)
    # Item 1 should be evicted (was least recently used)
    assert cache.get(0) is not None
    assert cache.get(1) is None


def test_lru_cache_update_existing():
    """Test updating an existing key."""
    cache = LRUCache(max_size_mb=1)

    # Add item
    cache.put(0, b"old")
    assert cache.current_size == 3

    # Update with larger value
    cache.put(0, b"new_value")
    assert cache.current_size == 9
    assert cache.get(0) == b"new_value"

    # Update with smaller value
    cache.put(0, b"new")
    assert cache.current_size == 3
    assert cache.get(0) == b"new"


def test_lru_cache_stats():
    """Test cache statistics."""
    cache = LRUCache(max_size_mb=1)

    # Initially empty
    stats = cache.stats()
    assert stats["enabled"] is True
    assert stats["hit_rate"] == 0.0
    assert stats["hits"] == 0
    assert stats["misses"] == 0
    assert stats["size_mb"] == 0.0
    assert stats["items"] == 0

    # Add some items
    cache.put(0, b"x" * 1000)
    cache.put(1, b"y" * 2000)

    # Hit and miss
    cache.get(0)  # hit
    cache.get(2)  # miss
    cache.get(0)  # hit

    stats = cache.stats()
    assert stats["hits"] == 2
    assert stats["misses"] == 1
    assert stats["hit_rate"] == 2.0 / 3.0
    assert stats["items"] == 2
    assert stats["size_mb"] > 0


def test_lru_cache_clear():
    """Test clearing the cache."""
    cache = LRUCache(max_size_mb=1)

    # Add items
    cache.put(0, b"test")
    cache.put(1, b"data")

    assert cache.current_size > 0
    assert len(cache.cache) > 0

    # Clear
    cache.clear()

    assert cache.current_size == 0
    assert len(cache.cache) == 0
    assert cache.get(0) is None


def test_lru_cache_large_item():
    """Test adding an item larger than cache size."""
    cache = LRUCache(max_size_mb=0.001)  # ~1KB

    # Add item larger than cache
    large_data = b"x" * 5000
    cache.put(0, large_data)

    # Cache should evict everything and be empty
    assert len(cache.cache) == 0
    assert cache.current_size == 0


def test_lru_cache_empty_data():
    """Test caching empty bytes."""
    cache = LRUCache(max_size_mb=1)

    cache.put(0, b"")
    assert cache.get(0) == b""
    assert cache.current_size == 0


def test_lru_cache_multiple_evictions():
    """Test that multiple items are evicted when needed."""
    cache = LRUCache(max_size_mb=0.001)  # ~1KB

    # Add 5 small items
    for i in range(5):
        cache.put(i, b"x" * 200)

    # Add one large item that requires evicting multiple items
    cache.put(10, b"y" * 800)

    # Most early items should be evicted
    evicted_count = sum(1 for i in range(5) if cache.get(i) is None)
    assert evicted_count >= 3  # At least 3 items evicted

    # Recent large item should be present
    assert cache.get(10) is not None
