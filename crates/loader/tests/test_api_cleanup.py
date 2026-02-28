"""Test the cleaned API to ensure functionality and no performance regression."""

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
    assert hasattr(hexz, "SignatureError")

    print("  All imports successful")


def test_no_underscore_names_in_dir():
    """Test that dir() on public modules shows no underscore-prefixed names."""
    import hexz
    import hexz.checkpoint

    hexz_dir = dir(hexz)
    underscore_names = [n for n in hexz_dir if n.startswith("_")]
    assert underscore_names == [], (
        f"dir(hexz) contains underscore names: {underscore_names}"
    )

    checkpoint_dir = dir(hexz.checkpoint)
    underscore_names = [n for n in checkpoint_dir if n.startswith("_")]
    assert underscore_names == [], (
        f"dir(hexz.checkpoint) contains underscore names: {underscore_names}"
    )

    print("  No underscore names in dir() output")


def test_metadata_dict_like():
    """Test that Metadata supports dict-like operations."""
    import hexz

    with tempfile.TemporaryDirectory() as tmpdir:
        snap_path = Path(tmpdir) / "test.hxz"
        data_file = Path(tmpdir) / "data.bin"
        data_file.write_bytes(b"test data" * 100000)

        with hexz.open(str(snap_path), mode="w") as writer:
            writer.add_file(str(data_file))

        meta = hexz.inspect(str(snap_path))

        # __contains__
        assert "version" in meta
        assert "nonexistent_key" not in meta

        # get()
        assert meta.get("version") is not None
        assert meta.get("nonexistent_key") is None
        assert meta.get("nonexistent_key", 42) == 42

        # keys()
        keys = list(meta.keys())
        assert "version" in keys

        print("  Metadata dict-like operations work")


def test_reader_len():
    """Test that len(reader) returns uncompressed size."""
    import hexz

    with tempfile.TemporaryDirectory() as tmpdir:
        snap_path = Path(tmpdir) / "test.hxz"
        data_file = Path(tmpdir) / "data.bin"
        test_data = b"test data" * 100000
        data_file.write_bytes(test_data)

        with hexz.open(str(snap_path), mode="w") as writer:
            writer.add_file(str(data_file))

        with hexz.open(str(snap_path)) as reader:
            assert len(reader) == reader.size
            assert len(reader) == len(test_data)

        print("  len(reader) works")


def test_metadata_consolidation():
    """Test that Metadata has the consolidated methods."""
    import hexz

    with tempfile.TemporaryDirectory() as tmpdir:
        snap_path = Path(tmpdir) / "test.hxz"
        data_file = Path(tmpdir) / "data.bin"
        data_file.write_bytes(b"test data" * 100000)

        with hexz.open(str(snap_path), mode="w") as writer:
            writer.add_file(str(data_file))

        meta = hexz.inspect(str(snap_path))

        str_output = str(meta)
        assert "Hexz Snapshot" in str_output
        assert str(snap_path) in str_output

        meta.print()

        snap_path2 = Path(tmpdir) / "test2.hxz"
        with hexz.open(str(snap_path2), mode="w") as writer:
            writer.add_file(str(data_file))

        diff_result = hexz.Metadata.diff(str(snap_path), str(snap_path2))
        assert isinstance(diff_result, dict)

        print("  Metadata consolidation works")


def test_reader_analyze():
    """Test that Reader has analyze() method."""
    import hexz

    with tempfile.TemporaryDirectory() as tmpdir:
        snap_path = Path(tmpdir) / "test.hxz"
        data_file = Path(tmpdir) / "data.bin"
        data_file.write_bytes(b"A" * 50000 + b"B" * 50000)

        with hexz.open(str(snap_path), mode="w") as writer:
            writer.add_file(str(data_file))

        with hexz.open(str(snap_path)) as reader:
            report = reader.analyze()
            assert hasattr(report, "dedup_ratio")
            assert hasattr(report, "savings_percent")
            assert report.total_bytes > 0

        print("  Reader.analyze() works")


def test_removed_functions():
    """Test that old deprecated functions are no longer available."""
    import hexz

    assert not hasattr(hexz, "info")
    assert not hasattr(hexz, "analyze")
    assert not hasattr(hexz, "diff")
    assert not hasattr(hexz, "merge_overlay")
    assert not hasattr(hexz, "unmount")
    assert not hasattr(hexz, "MountPoint")
    assert not hasattr(hexz, "pack")
    assert not hasattr(hexz, "snapshot_vm")
    assert not hasattr(hexz, "keygen")
    assert not hasattr(hexz, "sign_image")
    assert not hasattr(hexz, "verify_image")

    print("  Old functions properly removed")


def test_performance_no_regression():
    """Test that API changes didn't introduce performance regressions."""
    import hexz

    with tempfile.TemporaryDirectory() as tmpdir:
        snap_path = Path(tmpdir) / "perf.hxz"
        data_file = Path(tmpdir) / "data.bin"

        test_data = b"performance test data " * 500000
        data_file.write_bytes(test_data)

        start = time.time()
        with hexz.open(str(snap_path), mode="w", compression="lz4") as writer:
            writer.add_file(str(data_file))
        write_time = time.time() - start

        start = time.time()
        with hexz.open(str(snap_path)) as reader:
            data = reader.read()
        read_time = time.time() - start

        assert len(data) == len(test_data)
        assert write_time < 5.0, f"Write too slow: {write_time:.2f}s"
        assert read_time < 5.0, f"Read too slow: {read_time:.2f}s"

        print(f"  Performance: Write {write_time:.3f}s, Read {read_time:.3f}s")


def test_api_count():
    """Verify that we have the expected number of public API items."""
    import hexz

    public_items = [name for name in dir(hexz) if not name.startswith("_")]

    expected_core = [
        "open",
        "version",
        "Reader",
        "AsyncReader",
        "Writer",
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
        "SignatureError",
        "CacheError",
        "VersionError",
    ]

    for item in expected_core:
        assert item in public_items, f"Missing: {item}"

    print(f"  API surface: {len(hexz.__all__)} items in __all__")
    print(f"  Expected: {len(expected_core)} core items")


def test_mount_context_name():
    """Test that MountContext has a public name."""
    from hexz.mount import MountContext

    assert MountContext is not None
    print("  MountContext is publicly accessible")


def test_writer_mode_deprecation():
    """Test that using 'mode' parameter triggers a deprecation warning."""
    import warnings
    import hexz

    with tempfile.TemporaryDirectory() as tmpdir:
        snap_path = Path(tmpdir) / "test.hxz"
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            writer = hexz.Writer(str(snap_path), mode="fast")
            assert len(w) == 1
            assert issubclass(w[0].category, DeprecationWarning)
            assert "mode" in str(w[0].message)
            writer.finalize()

        # Using 'packing' should not warn
        snap_path2 = Path(tmpdir) / "test2.hxz"
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            writer = hexz.Writer(str(snap_path2), packing="fast")
            deprecation_warnings = [
                x for x in w if issubclass(x.category, DeprecationWarning)
            ]
            assert len(deprecation_warnings) == 0
            writer.finalize()

        print("  Writer 'mode' deprecation warning works")


if __name__ == "__main__":
    print("Testing Python API cleanup...")
    print()

    try:
        test_imports()
        test_no_underscore_names_in_dir()
        test_metadata_dict_like()
        test_reader_len()
        test_metadata_consolidation()
        test_reader_analyze()
        test_removed_functions()
        test_performance_no_regression()
        test_api_count()
        test_mount_context_name()
        test_writer_mode_deprecation()

        print()
        print("=" * 60)
        print("All tests passed! API cleanup successful with no regressions.")
        print("=" * 60)
        sys.exit(0)

    except Exception as e:
        print()
        print("=" * 60)
        print(f"Test failed: {e}")
        print("=" * 60)
        import traceback

        traceback.print_exc()
        sys.exit(1)
