"""Tests for checksum verification on read and strata.verify(checksum=True)."""

import os
import tempfile
import shutil
import pytest
import strata

# Header is 4096 bytes; first block data starts immediately after (Python writer and CLI pack).
HEADER_SIZE = 4096


@pytest.fixture
def temp_dir():
    d = tempfile.mkdtemp()
    try:
        yield d
    finally:
        shutil.rmtree(d, ignore_errors=True)


@pytest.fixture
def valid_snapshot(temp_dir):
    """A small valid snapshot with checksums (Python writer)."""
    data_path = os.path.join(temp_dir, "data.bin")
    snap_path = os.path.join(temp_dir, "snap.st")
    data = bytes([i % 256 for i in range(128 * 1024)])  # 128KB
    with open(data_path, "wb") as f:
        f.write(data)
    with strata.open(snap_path, mode="w", compression="lz4") as w:
        w.add(data_path)
    return snap_path, data


def test_verify_checksum_and_structure_returns_true_for_valid_snapshot(valid_snapshot):
    """strata.verify(path, checksum=True, structure=True) returns True when snapshot is valid."""
    snap_path, _ = valid_snapshot
    assert strata.verify(snap_path, checksum=True, structure=True) is True
    assert strata.verify(snap_path, checksum=True, structure=False) is True
    assert strata.verify(snap_path, checksum=False, structure=True) is True


def test_verify_structure_fails_for_invalid_path():
    """strata.verify with structure=True returns False for missing/invalid file."""
    assert (
        strata.verify("/nonexistent/path.st", structure=True, checksum=False) is False
    )


def test_verify_checksum_fails_for_corrupted_block(valid_snapshot, temp_dir):
    """When a block's data is corrupted on disk, read fails with checksum error and verify returns False."""
    snap_path, original_data = valid_snapshot
    corrupted_path = os.path.join(temp_dir, "corrupted.st")
    shutil.copy(snap_path, corrupted_path)

    # Corrupt first block: first block data starts at HEADER_SIZE
    with open(corrupted_path, "r+b") as f:
        f.seek(HEADER_SIZE + 1)
        b = f.read(1)
        f.seek(HEADER_SIZE + 1)
        f.write(bytes([b[0] ^ 0xFF]))

    # verify() should return False (checksum failure when reading through)
    assert strata.verify(corrupted_path, checksum=True, structure=True) is False

    # Direct read should raise (error message should indicate corruption/checksum)
    with strata.open(corrupted_path) as reader:
        with pytest.raises(OSError) as exc_info:
            reader.read(64 * 1024)  # read first block
        assert (
            "checksum" in str(exc_info.value).lower()
            or "corrupt" in str(exc_info.value).lower()
        )


def test_read_valid_snapshot_still_works_with_checksum_verification(valid_snapshot):
    """Reading a valid snapshot returns correct data (checksum verification is transparent)."""
    snap_path, expected = valid_snapshot
    with strata.open(snap_path) as reader:
        data = b"".join(reader.iter_chunks(chunk_size=32 * 1024))
    assert data == expected
