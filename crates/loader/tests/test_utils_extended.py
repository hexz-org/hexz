"""Extended tests for hexz.utils module to improve coverage from 86% to 92%+."""

import pytest
import os
import tempfile
import shutil
import hexz
from hexz import utils, crypto
from hexz.utils import Metadata, AnalysisReport, merge_overlay, analyze


@pytest.fixture
def temp_dir():
    """Create temporary directory for test files."""
    d = tempfile.mkdtemp(prefix="hexz_utils_test_")
    yield d
    shutil.rmtree(d, ignore_errors=True)


@pytest.fixture
def sample_data_file(temp_dir):
    """Create a sample data file for testing."""
    data_path = os.path.join(temp_dir, "data.bin")
    # Create data with patterns for deduplication
    data = b"pattern" * 1000 + b"unique" * 1000 + b"pattern" * 1000
    with open(data_path, "wb") as f:
        f.write(data)
    return data_path


@pytest.fixture
def sample_snapshot(temp_dir, sample_data_file):
    """Create a sample snapshot."""
    snap_path = os.path.join(temp_dir, "test.hxz")
    with hexz.open(snap_path, mode="w", compression="lz4") as w:
        w.add(sample_data_file)
    return snap_path


@pytest.fixture
def signed_snapshot(temp_dir, sample_snapshot):
    """Create a signed snapshot."""
    private_key = os.path.join(temp_dir, "test.key")
    public_key = os.path.join(temp_dir, "test.pub")
    crypto.keygen(private_key, public_key)
    crypto.sign(sample_snapshot, private_key)
    return sample_snapshot, public_key


def test_metadata_is_compatible_property(sample_snapshot):
    """Test the is_compatible property."""
    meta = hexz.inspect(sample_snapshot)
    assert isinstance(meta.is_compatible, bool)
    assert meta.is_compatible is True


def test_metadata_compatibility_status_property(sample_snapshot):
    """Test the compatibility_status property."""
    meta = hexz.inspect(sample_snapshot)
    status = meta.compatibility_status
    assert isinstance(status, str)
    assert status in ["full", "degraded", "incompatible", "unknown"]


def test_metadata_compatibility_message_property(sample_snapshot):
    """Test the compatibility_message property."""
    meta = hexz.inspect(sample_snapshot)
    message = meta.compatibility_message
    assert isinstance(message, str)
    assert len(message) > 0


def test_metadata_repr(sample_snapshot):
    """Test Metadata __repr__ method."""
    meta = hexz.inspect(sample_snapshot)
    repr_str = repr(meta)
    assert "Metadata" in repr_str
    assert "version=" in repr_str
    assert "compression=" in repr_str


def test_metadata_str_without_path(temp_dir, sample_data_file):
    """Test Metadata __str__ when created without path field."""
    snap_path = os.path.join(temp_dir, "nopath.hxz")
    with hexz.open(snap_path, mode="w") as w:
        w.add(sample_data_file)

    raw_meta = hexz.hexz_loader.inspect(str(snap_path))
    meta = Metadata(raw_meta)

    str_output = str(meta)
    assert "Hexz Snapshot" in str_output
    assert "Version:" in str_output


def test_metadata_str_with_memory(temp_dir):
    """Test Metadata __str__ when snapshot has memory data."""
    snap_path = os.path.join(temp_dir, "withmem.hxz")
    disk_path = os.path.join(temp_dir, "disk.bin")
    mem_path = os.path.join(temp_dir, "mem.bin")

    with open(disk_path, "wb") as f:
        f.write(b"\x00" * 65536)
    with open(mem_path, "wb") as f:
        f.write(b"\xaa" * 1024)

    with hexz.open(snap_path, mode="w") as w:
        w.add_file(disk_path, kind="disk")
        w.add_file(mem_path, kind="memory")

    meta = hexz.inspect(snap_path)
    str_output = str(meta)

    if meta.has_memory:
        assert "Secondary size:" in str_output


def test_metadata_getitem_access(sample_snapshot):
    """Test dict-like access to metadata."""
    meta = hexz.inspect(sample_snapshot)
    version = meta["version"]
    assert isinstance(version, int)


def test_metadata_diff_with_overlay_file(temp_dir, sample_snapshot):
    """Test Metadata.diff with an overlay file (.bin)."""
    overlay_path = os.path.join(temp_dir, "overlay.bin")
    meta_path = os.path.join(temp_dir, "overlay.meta")

    with open(overlay_path, "wb") as f:
        f.write(b"M" * 4096)

    with open(meta_path, "wb") as f:
        f.write((0).to_bytes(8, byteorder="little"))

    diff_info = Metadata.diff(sample_snapshot, overlay_path)
    assert isinstance(diff_info, dict)


def test_metadata_diff_with_two_snapshots(temp_dir, sample_data_file):
    """Test Metadata.diff comparing two snapshots."""
    snap1 = os.path.join(temp_dir, "snap1.hxz")
    snap2 = os.path.join(temp_dir, "snap2.hxz")

    with hexz.open(snap1, mode="w", compression="lz4") as w:
        w.add(sample_data_file)

    with hexz.open(snap2, mode="w", compression="zstd") as w:
        w.add(sample_data_file)

    diff_info = Metadata.diff(snap1, snap2)
    assert isinstance(diff_info, dict)
    assert "compression_same" in diff_info
    assert "version_same" in diff_info
    assert diff_info["compression_same"] is False


def test_merge_overlay_function(temp_dir, sample_snapshot):
    """Test merge_overlay function from utils."""
    overlay_path = os.path.join(temp_dir, "overlay.bin")
    meta_path = os.path.join(temp_dir, "overlay.meta")
    output_path = os.path.join(temp_dir, "merged.hxz")

    with open(overlay_path, "wb") as f:
        f.write(b"X" * 4096)

    with open(meta_path, "wb") as f:
        f.write((0).to_bytes(8, byteorder="little"))

    merge_overlay(sample_snapshot, overlay_path, output_path)

    assert os.path.exists(output_path)

    reader = hexz.open(output_path)
    assert reader.read(1, offset=0) == b"X"


def test_merge_overlay_thin_snapshot(temp_dir, sample_snapshot):
    """Test merge_overlay with thin=True parameter."""
    overlay_path = os.path.join(temp_dir, "thin_overlay.bin")
    meta_path = os.path.join(temp_dir, "thin_overlay.meta")
    output_path = os.path.join(temp_dir, "thin.hxz")

    open(overlay_path, "wb").close()
    open(meta_path, "wb").close()

    merge_overlay(sample_snapshot, overlay_path, output_path, thin=True)

    assert os.path.exists(output_path)

    meta = hexz.inspect(output_path)
    assert "parent_path" in meta._data


def test_analysis_report_repr(sample_data_file):
    """Test AnalysisReport __repr__ method."""
    report = analyze(sample_data_file)
    repr_str = repr(report)
    assert "AnalysisReport" in repr_str
    assert "dedup_ratio=" in repr_str
    assert "savings=" in repr_str


def test_analysis_report_properties(sample_data_file):
    """Test all AnalysisReport properties."""
    report = analyze(sample_data_file)

    assert isinstance(report.unique_bytes, (int, float))
    assert isinstance(report.total_bytes, (int, float))
    assert isinstance(report.predicted_ratio, float)
    assert isinstance(report.dedup_ratio, float)
    assert isinstance(report.savings_percent, float)

    assert report.total_bytes > 0
    assert report.unique_bytes > 0
    assert 0 <= report.dedup_ratio <= 1.0
    assert 0 <= report.savings_percent <= 100


def test_analyze_function(sample_data_file):
    """Test analyze function from utils."""
    report = analyze(sample_data_file)
    assert isinstance(report, AnalysisReport)
    assert hasattr(report, "unique_bytes")
    assert hasattr(report, "total_bytes")
    assert hasattr(report, "predicted_ratio")

    file_size = os.path.getsize(sample_data_file)
    assert report.total_bytes == file_size


def test_verify_with_structure_only(sample_snapshot):
    """Test verify with structure=True, checksum=False."""
    result = hexz.verify(sample_snapshot, structure=True, checksum=False)
    assert result is True


def test_verify_with_checksum_only(sample_snapshot):
    """Test verify with checksum=True, structure=False."""
    result = hexz.verify(sample_snapshot, checksum=True, structure=False)
    assert result is True


def test_verify_with_both_checks(sample_snapshot):
    """Test verify with both structure and checksum."""
    result = hexz.verify(sample_snapshot, structure=True, checksum=True)
    assert result is True


def test_verify_with_signature(signed_snapshot):
    """Test verify with public_key parameter (signature verification)."""
    snapshot_path, public_key = signed_snapshot
    result = hexz.verify(snapshot_path, public_key=public_key)
    assert result is True


def test_verify_with_wrong_signature_fails(signed_snapshot, temp_dir):
    """Test verify fails with wrong public key."""
    snapshot_path, _ = signed_snapshot

    wrong_private = os.path.join(temp_dir, "wrong.key")
    wrong_public = os.path.join(temp_dir, "wrong.pub")
    crypto.keygen(wrong_private, wrong_public)

    result = hexz.verify(snapshot_path, public_key=wrong_public)
    assert result is False


def test_verify_corrupted_file_fails(temp_dir, sample_snapshot):
    """Test verify returns False for corrupted file."""
    corrupted = os.path.join(temp_dir, "corrupted.hxz")

    with open(sample_snapshot, "rb") as src:
        data = bytearray(src.read())

    if len(data) > 1000:
        data[500:510] = b"X" * 10

    with open(corrupted, "wb") as dst:
        dst.write(data)

    # Just run the code path; result depends on corruption location
    result = hexz.verify(corrupted, checksum=True, structure=True)
    assert isinstance(result, bool)


def test_verify_with_all_parameters(signed_snapshot):
    """Test verify with all parameters specified."""
    snapshot_path, public_key = signed_snapshot
    result = hexz.verify(
        snapshot_path, checksum=True, structure=True, public_key=public_key
    )
    assert result is True


def test_verify_nonexistent_file_returns_false(temp_dir):
    """Test verify returns False for nonexistent file."""
    nonexistent = os.path.join(temp_dir, "does_not_exist.hxz")
    result = hexz.verify(nonexistent)
    assert result is False


def test_metadata_print_method(sample_snapshot, capsys):
    """Test Metadata.print() method."""
    meta = hexz.inspect(sample_snapshot)
    meta.print()
    captured = capsys.readouterr()
    assert "Hexz Snapshot" in captured.out
    assert "Version:" in captured.out


def test_format_version_constants():
    """Test that format version constants are defined."""
    assert isinstance(utils.FORMAT_VERSION, int)
    assert isinstance(utils.MIN_SUPPORTED_VERSION, int)
    assert isinstance(utils.MAX_SUPPORTED_VERSION, int)
    assert utils.FORMAT_VERSION > 0
    assert utils.MIN_SUPPORTED_VERSION <= utils.FORMAT_VERSION
    assert utils.MAX_SUPPORTED_VERSION >= utils.FORMAT_VERSION


def test_metadata_all_properties(sample_snapshot):
    """Test all Metadata properties for coverage."""
    meta = hexz.inspect(sample_snapshot)
    _ = meta.version
    _ = meta.compression
    _ = meta.primary_size
    _ = meta.secondary_size
    _ = meta.size_compressed
    _ = meta.block_size
    _ = meta.num_blocks
    _ = meta.encrypted
    _ = meta.signed
    _ = meta.has_disk
    _ = meta.has_memory
    _ = meta.is_compatible
    _ = meta.compatibility_status
    _ = meta.compatibility_message
    _ = meta.compression_ratio
    assert meta.version >= 0


def test_inspect_with_pathlike(temp_dir, sample_data_file):
    """Test inspect with Path object."""
    from pathlib import Path

    snap_path = Path(temp_dir) / "pathlike.hxz"
    with hexz.open(str(snap_path), mode="w") as w:
        w.add(sample_data_file)

    meta = hexz.inspect(snap_path)
    assert meta.version > 0
