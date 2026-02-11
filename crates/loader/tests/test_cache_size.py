"""Tests for cache_size parameter functionality."""

import pytest
import strata
from strata.reader import _parse_cache_size


def test_parse_cache_size_bytes():
    """Test parsing plain byte values."""
    assert _parse_cache_size("1024") == 1024
    assert _parse_cache_size("1048576") == 1048576


def test_parse_cache_size_kilobytes():
    """Test parsing kilobyte values."""
    assert _parse_cache_size("1K") == 1024
    assert _parse_cache_size("512K") == 512 * 1024
    assert _parse_cache_size("1KB") == 1024
    assert _parse_cache_size("1KiB") == 1024


def test_parse_cache_size_megabytes():
    """Test parsing megabyte values."""
    assert _parse_cache_size("1M") == 1024 * 1024
    assert _parse_cache_size("512M") == 512 * 1024 * 1024
    assert _parse_cache_size("1MB") == 1024 * 1024
    assert _parse_cache_size("1MiB") == 1024 * 1024


def test_parse_cache_size_gigabytes():
    """Test parsing gigabyte values."""
    assert _parse_cache_size("1G") == 1024 * 1024 * 1024
    assert _parse_cache_size("2G") == 2 * 1024 * 1024 * 1024
    assert _parse_cache_size("1GB") == 1024 * 1024 * 1024
    assert _parse_cache_size("1GiB") == 1024 * 1024 * 1024


def test_parse_cache_size_terabytes():
    """Test parsing terabyte values."""
    assert _parse_cache_size("1T") == 1024 * 1024 * 1024 * 1024
    assert _parse_cache_size("1TB") == 1024 * 1024 * 1024 * 1024


def test_parse_cache_size_whitespace():
    """Test that whitespace is handled correctly."""
    assert _parse_cache_size("  512M  ") == 512 * 1024 * 1024
    assert _parse_cache_size("1 G") == 1024 * 1024 * 1024


def test_parse_cache_size_case_insensitive():
    """Test that parsing is case-insensitive."""
    assert _parse_cache_size("512m") == 512 * 1024 * 1024
    assert _parse_cache_size("1g") == 1024 * 1024 * 1024
    assert _parse_cache_size("1gb") == 1024 * 1024 * 1024


def test_parse_cache_size_invalid():
    """Test that invalid formats raise ValueError."""
    with pytest.raises(ValueError):
        _parse_cache_size("invalid")

    with pytest.raises(ValueError):
        _parse_cache_size("512X")

    with pytest.raises(ValueError):
        _parse_cache_size("")


def test_reader_with_cache_size(base_snap_path):
    """Test that Reader accepts cache_size parameter."""
    # Should not raise an error
    reader = strata.Reader(base_snap_path, cache_size="512M")
    assert reader is not None

    # Read some data to verify it works
    data = reader.read(1024)
    assert len(data) == 1024


def test_reader_with_different_cache_sizes(base_snap_path, raw_data_path):
    """Test that different cache sizes work correctly."""
    # Read raw data for comparison
    with open(raw_data_path, "rb") as f:
        raw_data = f.read()

    # Test with small cache
    reader_small = strata.Reader(base_snap_path, cache_size="64K")
    data_small = reader_small.read(4096)
    assert data_small == raw_data[:4096]

    # Test with large cache
    reader_large = strata.Reader(base_snap_path, cache_size="10M")
    data_large = reader_large.read(4096)
    assert data_large == raw_data[:4096]

    # Test with no cache_size (default)
    reader_default = strata.Reader(base_snap_path)
    data_default = reader_default.read(4096)
    assert data_default == raw_data[:4096]


def test_open_with_cache_size(base_snap_path):
    """Test that strata.open() accepts cache_size via Reader."""
    with strata.open(base_snap_path) as reader:
        # Default cache size
        data = reader.read(1024)
        assert len(data) == 1024
