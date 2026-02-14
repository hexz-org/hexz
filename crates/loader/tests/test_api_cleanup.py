"""Test the new cleaned API to ensure functionality and no performance regression."""

import sys
import tempfile
import time
from pathlib import Path


# Test that imports work correctly
def test_imports():
    """Test that all new API functions are importable."""
    import hexz

    # Core I/O
    assert hasattr(hexz, "open")
    assert hasattr(hexz, "version")
    assert hasattr(hexz, "Reader")
    assert hasattr(hexz, "AsyncReader")
    assert hasattr(hexz, "Writer")

    # ML
    assert hasattr(hexz, "Dataset")
    assert hasattr(hexz, "TFDataset")

    # Arrays
    assert hasattr(hexz, "read_array")
    assert hasattr(hexz, "write_array")
    assert hasattr(hexz, "ArrayView")

    # Build
    assert hasattr(hexz, "build")
    assert hasattr(hexz, "PROFILES")

    # Inspection
    assert hasattr(hexz, "inspect")

    # Mount
    assert hasattr(hexz, "mount")

    # Verify
    assert hasattr(hexz, "verify")

    # Crypto submodule
    assert hasattr(hexz, "crypto")
    assert hasattr(hexz.crypto, "keygen")
    assert hasattr(hexz.crypto, "sign")
    assert hasattr(hexz.crypto, "verify")

    # Types
    assert hasattr(hexz, "Metadata")
    assert hasattr(hexz, "PathLike")

    # Version constants
    assert hasattr(hexz, "FORMAT_VERSION")
    assert hasattr(hexz, "MIN_SUPPORTED_VERSION")
    assert hasattr(hexz, "MAX_SUPPORTED_VERSION")

    # Exceptions
    assert hasattr(hexz, "Error")
    assert hasattr(hexz, "IOError")

    print("✓ All imports successful")


def test_metadata_consolidation():
    """Test that Metadata has the new consolidated methods."""
    import hexz

    # Create a temporary snapshot for testing
    with tempfile.TemporaryDirectory() as tmpdir:
        snap_path = Path(tmpdir) / "test.hxz"
        data_file = Path(tmpdir) / "data.bin"

        # Create test data (1 MB)
        data_file.write_bytes(b"test data" * 100000)

        # Build snapshot
        with hexz.open(str(snap_path), mode="w") as writer:
            writer.add_file(str(data_file))

        # Test new Metadata API
        meta = hexz.inspect(str(snap_path))

        # Test __str__ (should work without error)
        str_output = str(meta)
        assert "Hexz Snapshot" in str_output
        assert str(snap_path) in str_output

        # Test print() method
        meta.print()  # Should not raise

        # Test diff() classmethod
        snap_path2 = Path(tmpdir) / "test2.hxz"
        with hexz.open(str(snap_path2), mode="w") as writer:
            writer.add_file(str(data_file))

        diff_result = hexz.Metadata.diff(str(snap_path), str(snap_path2))
        assert isinstance(diff_result, dict)

        print("✓ Metadata consolidation works")


def test_reader_analyze():
    """Test that Reader has analyze() method."""
    import hexz

    with tempfile.TemporaryDirectory() as tmpdir:
        snap_path = Path(tmpdir) / "test.hxz"
        data_file = Path(tmpdir) / "data.bin"

        # Create test data with some repetition for dedup
        data_file.write_bytes(b"A" * 50000 + b"B" * 50000)

        # Build snapshot
        with hexz.open(str(snap_path), mode="w") as writer:
            writer.add_file(str(data_file))

        # Test analyze() method
        with hexz.open(str(snap_path)) as reader:
            report = reader.analyze()
            assert hasattr(report, "dedup_ratio")
            assert hasattr(report, "savings_percent")
            assert report.total_bytes > 0

        print("✓ Reader.analyze() works")


def test_writer_merge_overlay():
    """Test that Writer has merge_overlay() method."""
    import hexz

    with tempfile.TemporaryDirectory() as tmpdir:
        base_path = Path(tmpdir) / "base.hxz"
        overlay_path = Path(tmpdir) / "overlay.img"
        merged_path = Path(tmpdir) / "merged.hxz"
        data_file = Path(tmpdir) / "data.bin"

        # Create base snapshot
        data_file.write_bytes(b"base data" * 10000)
        with hexz.open(str(base_path), mode="w") as writer:
            writer.add_file(str(data_file))

        # Create overlay (simulate modified blocks)
        overlay_path.write_bytes(b"overlay" * 10000)
        meta_path = overlay_path.with_suffix(".meta")
        meta_path.write_bytes(b"")  # Empty metadata file

        # Test merge_overlay() method
        with hexz.open(str(merged_path), mode="w") as writer:
            writer.merge_overlay(
                base=str(base_path), overlay=str(overlay_path), thin=False
            )

        assert merged_path.exists()
        print("✓ Writer.merge_overlay() works")


def test_removed_functions():
    """Test that old deprecated functions are no longer available."""
    import hexz

    # These should NOT be in the API anymore
    assert not hasattr(hexz, "info")  # Use print(inspect(...)) or inspect().print()
    assert not hasattr(hexz, "analyze")  # Use reader.analyze()
    assert not hasattr(hexz, "diff")  # Use Metadata.diff()
    assert not hasattr(hexz, "merge_overlay")  # Use writer.merge_overlay()
    assert not hasattr(hexz, "unmount")  # Use mount() context manager
    assert not hasattr(hexz, "MountPoint")  # Internal class
    assert not hasattr(hexz, "pack")  # Use build() or Writer
    assert not hasattr(hexz, "snapshot_vm")  # CLI-only
    assert not hasattr(hexz, "keygen")  # Use crypto.keygen
    assert not hasattr(hexz, "sign_image")  # Use crypto.sign
    assert not hasattr(hexz, "verify_image")  # Use crypto.verify or verify()

    print("✓ Old functions properly removed")


def test_performance_no_regression():
    """Test that API changes didn't introduce performance regressions."""
    import hexz

    with tempfile.TemporaryDirectory() as tmpdir:
        snap_path = Path(tmpdir) / "perf.hxz"
        data_file = Path(tmpdir) / "data.bin"

        # Create 10 MB test file
        test_data = b"performance test data " * 500000
        data_file.write_bytes(test_data)

        # Test write performance
        start = time.time()
        with hexz.open(str(snap_path), mode="w", compression="lz4") as writer:
            writer.add_file(str(data_file))
        write_time = time.time() - start

        # Test read performance
        start = time.time()
        with hexz.open(str(snap_path)) as reader:
            data = reader.read()
        read_time = time.time() - start

        # Verify data integrity
        assert len(data) == len(test_data)

        # Performance should be reasonable (< 5 seconds for 10 MB)
        assert write_time < 5.0, f"Write too slow: {write_time:.2f}s"
        assert read_time < 5.0, f"Read too slow: {read_time:.2f}s"

        print(f"✓ Performance: Write {write_time:.3f}s, Read {read_time:.3f}s")


def test_api_count():
    """Verify that we have the expected number of public API items."""
    import hexz

    # Count public exports (should be ~30)
    public_items = [name for name in dir(hexz) if not name.startswith("_")]

    # Expected core items from __all__
    expected_core = [
        "open",
        "version",
        "Reader",
        "AsyncReader",
        "Writer",
        "Dataset",
        "TFDataset",
        "read_array",
        "write_array",
        "ArrayView",
        "build",
        "PROFILES",
        "inspect",
        "mount",
        "verify",
        "crypto",
        "Metadata",
        "PathLike",
        "FORMAT_VERSION",
        "MIN_SUPPORTED_VERSION",
        "MAX_SUPPORTED_VERSION",
        # Exceptions
        "Error",
        "IOError",
        "NetworkError",
        "FormatError",
        "ValidationError",
        "CompressionError",
        "EncryptionError",
        "MountError",
        "CacheError",
        "VersionError",
    ]

    # All expected items should be present
    for item in expected_core:
        assert item in public_items, f"Missing: {item}"

    print(f"✓ API surface: {len(hexz.__all__)} items in __all__")
    print(f"  Expected: {len(expected_core)} core items")


if __name__ == "__main__":
    print("Testing Python API cleanup...")
    print()

    try:
        test_imports()
        test_metadata_consolidation()
        test_reader_analyze()
        test_writer_merge_overlay()
        test_removed_functions()
        test_performance_no_regression()
        test_api_count()

        print()
        print("=" * 60)
        print("✓ All tests passed! API cleanup successful with no regressions.")
        print("=" * 60)
        sys.exit(0)

    except Exception as e:
        print()
        print("=" * 60)
        print(f"✗ Test failed: {e}")
        print("=" * 60)
        import traceback

        traceback.print_exc()
        sys.exit(1)
