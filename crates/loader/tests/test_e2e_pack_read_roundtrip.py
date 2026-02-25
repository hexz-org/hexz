"""
E2E tests: pack with CLI → open with Python → read and verify data integrity.

Covers:
- Synthetic datasets (varying sizes and patterns)
- Full flow: hexz data pack → hexz.open() → read / iter_chunks → byte comparison
- LZ4 and Zstd compression
- Run in CI (hexz binary from target/release or target/debug, or PATH)
"""

import hashlib
import os
import shutil
import subprocess
import tempfile

import pytest

import hexz


def _hexz_binary():
    """Path to hexz CLI binary, or None if not found."""
    if shutil.which("hexz"):
        return "hexz"
    # Tests live in crates/loader/tests/; repo root has target/release/hexz
    test_dir = os.path.dirname(os.path.abspath(__file__))
    loader_root = os.path.dirname(test_dir)  # crates/loader
    crates_dir = os.path.dirname(loader_root)  # crates
    repo_root = os.path.dirname(crates_dir)  # repo root (has target/)
    for name in ("hexz", "hexz.exe"):
        for subdir in ("release", "debug"):
            path = os.path.join(repo_root, "target", subdir, name)
            if os.path.isfile(path) and os.access(path, os.X_OK):
                return path
    return None


@pytest.fixture(scope="module")
def hexz_binary():
    """Hexz CLI binary; skips tests if not found."""
    bin_path = _hexz_binary()
    if bin_path is None:
        pytest.skip(
            "hexz binary not found (build with: cargo build --release -p hexz-cli)"
        )
    return bin_path


@pytest.fixture
def e2e_temp_dir():
    """Temporary directory for E2E test artifacts."""
    tmp = tempfile.mkdtemp(prefix="hexz_e2e_")
    try:
        yield tmp
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def _make_synthetic_file(path: str, size: int, pattern: str = "deterministic") -> bytes:
    """
    Write a synthetic file and return its content for comparison.
    pattern: "deterministic" (repeating bytes), "random", "zeros"
    """
    if pattern == "zeros":
        data = bytes(size)
    elif pattern == "random":
        data = os.urandom(size)
    else:
        # deterministic: repeat 0..255 so compression gets something to work with
        data = bytes([i % 256 for i in range(size)])
    with open(path, "wb") as f:
        f.write(data)
    return data


def _pack_with_cli(
    hexz_bin: str, disk_path: str, output_path: str, compression: str
) -> None:
    """Run: hexz data pack --disk <disk_path>  <output_path> --compression <compression>."""
    cmd = [
        hexz_bin,
        "pack",
        "--disk",
        disk_path,
        output_path,
        "--compression",
        compression,
    ]
    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        timeout=120,
        cwd=os.path.dirname(disk_path) or ".",
    )
    assert result.returncode == 0, (
        f"hexz data pack failed: {result.stderr or result.stdout}"
    )


def _read_all_via_python(snap_path: str) -> bytes:
    """Open snapshot with hexz.open() and read all bytes via iter_chunks (then concat)."""
    with hexz.open(snap_path) as reader:
        chunks = list(reader.iter_chunks(chunk_size=256 * 1024))
    return b"".join(chunks)


def _read_all_via_read(snap_path: str) -> bytes:
    """Open snapshot and read all with reader.read(size) in a loop (alternative path)."""
    with hexz.open(snap_path) as reader:
        parts = []
        chunk_size = 256 * 1024
        while True:
            chunk = reader.read(chunk_size)
            if not chunk:
                break
            parts.append(chunk)
        return b"".join(parts)


# --- Tests ---


def test_e2e_pack_lz4_roundtrip(hexz_binary, e2e_temp_dir):
    """
    Pack a synthetic file with CLI (LZ4) → open with hexz.open() → read via iter_chunks → compare to original.
    """
    disk_path = os.path.join(e2e_temp_dir, "data.bin")
    snap_path = os.path.join(e2e_temp_dir, "out.hxz")
    expected = _make_synthetic_file(disk_path, 1024 * 1024, "deterministic")  # 1MB

    _pack_with_cli(hexz_binary, disk_path, snap_path, "lz4")

    actual = _read_all_via_python(snap_path)
    assert actual == expected, (
        f"Round-trip mismatch: len(expected)={len(expected)}, len(actual)={len(actual)}"
    )


def test_e2e_pack_zstd_roundtrip(hexz_binary, e2e_temp_dir):
    """
    Same as LZ4 but with --compression zstd.
    """
    disk_path = os.path.join(e2e_temp_dir, "data.bin")
    snap_path = os.path.join(e2e_temp_dir, "out.hxz")
    expected = _make_synthetic_file(disk_path, 512 * 1024, "deterministic")  # 512KB

    _pack_with_cli(hexz_binary, disk_path, snap_path, "zstd")

    actual = _read_all_via_python(snap_path)
    assert actual == expected


def test_e2e_pack_read_via_read_method(hexz_binary, e2e_temp_dir):
    """
    Verify the reader.read(size) path (not only iter_chunks) also sees correct data.
    """
    disk_path = os.path.join(e2e_temp_dir, "data.bin")
    snap_path = os.path.join(e2e_temp_dir, "out.hxz")
    expected = _make_synthetic_file(disk_path, 100 * 1024, "deterministic")  # 100KB

    _pack_with_cli(hexz_binary, disk_path, snap_path, "lz4")

    actual = _read_all_via_read(snap_path)
    assert actual == expected


@pytest.mark.parametrize(
    "size_kib,pattern",
    [
        (0, "zeros"),  # edge: empty
        (1, "deterministic"),  # 1KB
        (1024, "deterministic"),  # 1MB
        (100, "random"),  # 100KB random
    ],
)
def test_e2e_pack_varying_sizes_and_patterns(
    hexz_binary, e2e_temp_dir, size_kib, pattern
):
    """
    Pack and read back with varying sizes and content patterns; compare bytes.
    """
    size = size_kib * 1024
    disk_path = os.path.join(e2e_temp_dir, f"data_{size_kib}k_{pattern}.bin")
    snap_path = os.path.join(e2e_temp_dir, f"out_{size_kib}k_{pattern}.hxz")

    expected = _make_synthetic_file(disk_path, size, pattern)
    _pack_with_cli(hexz_binary, disk_path, snap_path, "lz4")

    actual = _read_all_via_python(snap_path)
    assert len(actual) == len(expected), (
        f"Size mismatch: {len(actual)} vs {len(expected)}"
    )
    assert actual == expected


def test_e2e_pack_checksum_consistency(hexz_binary, e2e_temp_dir):
    """
    After round-trip, compare SHA-256 of original and read-back data.
    """
    disk_path = os.path.join(e2e_temp_dir, "data.bin")
    snap_path = os.path.join(e2e_temp_dir, "out.hxz")
    expected = _make_synthetic_file(disk_path, 200 * 1024, "deterministic")

    _pack_with_cli(hexz_binary, disk_path, snap_path, "zstd")
    actual = _read_all_via_python(snap_path)

    expected_hash = hashlib.sha256(expected).hexdigest()
    actual_hash = hashlib.sha256(actual).hexdigest()
    assert actual_hash == expected_hash, (
        f"SHA-256 mismatch: {actual_hash} vs {expected_hash}"
    )


def test_e2e_inspect_after_pack(hexz_binary, e2e_temp_dir):
    """
    After packing with CLI, hexz.inspect() reports expected size and compression.
    """
    disk_path = os.path.join(e2e_temp_dir, "data.bin")
    snap_path = os.path.join(e2e_temp_dir, "out.hxz")
    size = 64 * 1024
    _make_synthetic_file(disk_path, size, "deterministic")
    _pack_with_cli(hexz_binary, disk_path, snap_path, "lz4")

    meta = hexz.inspect(snap_path)
    assert meta.primary_size == size
    assert meta.compression is not None
    assert meta.version >= 1
