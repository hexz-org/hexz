"""
Tests for profile-based snapshot building in strata.profiles.

This module tests the uncovered functionality in profiles.py:
- build() with file source
- build() with directory source
- build() with invalid source (error handling)
- build() with profile parameter overrides
- All available profiles
"""

import pytest
import tempfile
import os
import strata


def test_build_with_file_source():
    """Test build() with single file source."""
    with tempfile.TemporaryDirectory() as tmpdir:
        # Create input file
        input_file = os.path.join(tmpdir, "input.img")
        with open(input_file, "wb") as f:
            f.write(b"test data" * 1000)

        output_file = os.path.join(tmpdir, "output.st")

        # Build snapshot from single file
        meta = strata.build(input_file, output_file, profile="generic")

        assert meta is not None
        assert meta.size_compressed > 0
        assert os.path.exists(output_file)


def test_build_with_directory_source():
    """Test build() with directory source."""
    with tempfile.TemporaryDirectory() as tmpdir:
        # Create test directory with files
        test_dir = os.path.join(tmpdir, "test_dir")
        os.makedirs(test_dir)

        for i in range(5):
            file_path = os.path.join(test_dir, f"file{i}.dat")
            with open(file_path, "wb") as f:
                f.write(b"data" * 100)

        output_file = os.path.join(tmpdir, "output.st")

        # Build snapshot from directory
        meta = strata.build(test_dir, output_file, profile="ml")

        assert meta is not None
        assert meta.size_compressed > 0
        assert os.path.exists(output_file)


def test_build_invalid_source():
    """Test ValidationError on non-existent source."""
    with tempfile.TemporaryDirectory() as tmpdir:
        output_file = os.path.join(tmpdir, "output.st")
        nonexistent = os.path.join(tmpdir, "nonexistent")

        with pytest.raises(strata.ValidationError, match="Source not found"):
            strata.build(nonexistent, output_file)


def test_build_with_profile_overrides():
    """Test build() with profile parameter overrides."""
    with tempfile.TemporaryDirectory() as tmpdir:
        input_file = os.path.join(tmpdir, "input.img")
        with open(input_file, "wb") as f:
            f.write(b"test data" * 500)

        output_file = os.path.join(tmpdir, "output.st")

        # Build with profile and overrides
        meta = strata.build(
            input_file,
            output_file,
            profile="generic",
            compression="zstd",  # Override compression
            block_size=128 * 1024,  # Override block size
        )

        assert meta is not None
        assert meta.compression == "Zstd"  # Capitalized


def test_build_ml_profile():
    """Test build() with ML profile."""
    with tempfile.TemporaryDirectory() as tmpdir:
        input_file = os.path.join(tmpdir, "dataset.img")
        with open(input_file, "wb") as f:
            f.write(b"x" * 10000)

        output_file = os.path.join(tmpdir, "dataset.st")

        meta = strata.build(input_file, output_file, profile="ml")

        assert meta is not None
        assert meta.version == 1


def test_build_eda_profile():
    """Test build() with EDA profile."""
    with tempfile.TemporaryDirectory() as tmpdir:
        input_file = os.path.join(tmpdir, "data.img")
        with open(input_file, "wb") as f:
            f.write(b"y" * 10000)

        output_file = os.path.join(tmpdir, "data.st")

        meta = strata.build(input_file, output_file, profile="eda")

        assert meta is not None
        assert meta.version == 1


def test_build_embedded_profile():
    """Test build() with embedded profile."""
    with tempfile.TemporaryDirectory() as tmpdir:
        input_file = os.path.join(tmpdir, "firmware.img")
        with open(input_file, "wb") as f:
            f.write(b"z" * 5000)

        output_file = os.path.join(tmpdir, "firmware.st")

        meta = strata.build(input_file, output_file, profile="embedded")

        assert meta is not None
        assert meta.version == 1


def test_build_archival_profile():
    """Test build() with archival profile."""
    with tempfile.TemporaryDirectory() as tmpdir:
        input_file = os.path.join(tmpdir, "archive.img")
        with open(input_file, "wb") as f:
            f.write(b"a" * 10000)

        output_file = os.path.join(tmpdir, "archive.st")

        meta = strata.build(input_file, output_file, profile="archival")

        assert meta is not None
        assert meta.version == 1


def test_build_generic_profile():
    """Test build() with generic profile (default)."""
    with tempfile.TemporaryDirectory() as tmpdir:
        input_file = os.path.join(tmpdir, "generic.img")
        with open(input_file, "wb") as f:
            f.write(b"b" * 8000)

        output_file = os.path.join(tmpdir, "generic.st")

        meta = strata.build(input_file, output_file, profile="generic")

        assert meta is not None
        assert meta.version == 1


def test_build_default_profile():
    """Test build() without specifying profile (uses default)."""
    with tempfile.TemporaryDirectory() as tmpdir:
        input_file = os.path.join(tmpdir, "default.img")
        with open(input_file, "wb") as f:
            f.write(b"c" * 6000)

        output_file = os.path.join(tmpdir, "default.st")

        # Should use default profile (generic)
        meta = strata.build(input_file, output_file)

        assert meta is not None
        assert meta.version == 1


def test_build_creates_valid_snapshot():
    """Test that build() creates a valid, readable snapshot."""
    with tempfile.TemporaryDirectory() as tmpdir:
        input_file = os.path.join(tmpdir, "input.img")
        test_data = b"test data content" * 100
        with open(input_file, "wb") as f:
            f.write(test_data)

        output_file = os.path.join(tmpdir, "output.st")

        meta = strata.build(input_file, output_file, profile="ml")

        assert meta is not None

        # Verify the snapshot can be read
        reader = strata.Reader(output_file)
        assert reader.metadata.disk_size == len(test_data)

        # Read and verify data
        data = reader.read(len(test_data))
        assert data == test_data


def test_build_with_cdc_override():
    """Test build() with CDC enabled via override."""
    with tempfile.TemporaryDirectory() as tmpdir:
        input_file = os.path.join(tmpdir, "input.img")
        with open(input_file, "wb") as f:
            f.write(b"d" * 20000)

        output_file = os.path.join(tmpdir, "output.st")

        # Enable CDC via override
        meta = strata.build(
            input_file,
            output_file,
            profile="generic",
            cdc=True,
        )

        assert meta is not None
        assert os.path.exists(output_file)
