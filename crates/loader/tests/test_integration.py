import pytest
import os
import http.server
import socketserver
import threading
import tempfile
import shutil
import numpy as np
import tarfile
import strata
import io  # Added missing import

# --- Fixtures ---


@pytest.fixture
def temp_dir():
    d = tempfile.mkdtemp()
    yield d
    shutil.rmtree(d)


@pytest.fixture
def sample_snapshot(temp_dir):
    """Creates a simple snapshot with known data for testing."""
    snap_path = os.path.join(temp_dir, "test.st")
    data_path = os.path.join(temp_dir, "data.bin")

    # Create 1MB of deterministic data
    data = bytes([i % 256 for i in range(1024 * 1024)])
    with open(data_path, "wb") as f:
        f.write(data)

    with strata.open(snap_path, mode="w") as w:
        w.add(data_path)

    return snap_path, data


# --- Tests ---


def test_tarfile_integration(temp_dir):
    """
    Test that StrataReader behaves enough like a file object to work
    with Python's tarfile module (read-only).
    """
    # 1. Create a valid tar file
    tar_source = os.path.join(temp_dir, "source.tar")
    with tarfile.open(tar_source, "w") as tar:
        t = tarfile.TarInfo("hello.txt")
        t.size = 5
        tar.addfile(t, io.BytesIO(b"hello"))

    # 2. Wrap it in a Strata snapshot
    snap_path = os.path.join(temp_dir, "archive.st")
    with strata.open(snap_path, mode="w") as w:
        w.add(tar_source)

    # 3. Open snapshot via StrataReader and pass to tarfile
    reader = strata.open(snap_path)

    # tarfile needs a file-like object. StrataReader works.
    with tarfile.open(fileobj=reader, mode="r:") as tar:
        names = tar.getnames()
        assert "hello.txt" in names
        f = tar.extractfile("hello.txt")
        assert f.read() == b"hello"


def test_numpy_integration(sample_snapshot):
    snap_path, raw_data = sample_snapshot

    # Read a chunk as a numpy array using the helper
    # Offset 100, shape (10, 10), uint8 -> 100 bytes
    arr = strata.read_array(snap_path, offset=100, shape=(10, 10), dtype="uint8")

    assert arr.shape == (10, 10)
    assert arr.dtype == np.uint8

    expected = np.frombuffer(raw_data[100:200], dtype=np.uint8).reshape(10, 10)
    np.testing.assert_array_equal(arr, expected)


def test_http_backend(sample_snapshot):
    """
    Tests reading a snapshot over HTTP using a local background server.
    We implement a custom handler because SimpleHTTPRequestHandler does not
    support Range requests by default, which Strata requires.
    """
    snap_path, raw_data = sample_snapshot
    directory = os.path.dirname(snap_path)
    filename = os.path.basename(snap_path)

    class RangeRequestHandler(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *args, **kwargs):
            super().__init__(*args, directory=directory, **kwargs)

        def do_GET(self):
            # Basic Range support for testing
            if "Range" in self.headers:
                try:
                    # Parse "bytes=START-END"
                    range_header = self.headers["Range"]
                    if not range_header.startswith("bytes="):
                        self.send_error(400, "Invalid Range header")
                        return

                    start_str, end_str = range_header[6:].split("-")
                    start = int(start_str)

                    path = self.translate_path(self.path)
                    file_size = os.path.getsize(path)

                    if end_str:
                        end = int(end_str)
                    else:
                        end = file_size - 1

                    # Clamp end to file size
                    if end >= file_size:
                        end = file_size - 1

                    length = end - start + 1

                    if start >= file_size:
                        self.send_error(416, "Range Not Satisfiable")
                        return

                    self.send_response(206)
                    self.send_header("Content-Type", "application/octet-stream")
                    self.send_header(
                        "Content-Range", f"bytes {start}-{end}/{file_size}"
                    )
                    self.send_header("Content-Length", str(length))
                    self.end_headers()

                    with open(path, "rb") as f:
                        f.seek(start)
                        self.wfile.write(f.read(length))
                    return
                except ValueError:
                    pass

            # Fallback for non-range requests (e.g. HEAD or full GET)
            super().do_GET()

    # Bind to port 0 to get a free port
    httpd = socketserver.TCPServer(("127.0.0.1", 0), RangeRequestHandler)
    port = httpd.server_address[1]

    server_thread = threading.Thread(target=httpd.serve_forever)
    server_thread.daemon = True
    server_thread.start()

    try:
        url = f"http://127.0.0.1:{port}/{filename}"

        # Open via Strata with HTTP URL
        # allow_restricted=True is required for localhost IPs
        reader = strata.open(url, allow_restricted=True)

        assert reader.size == len(raw_data)

        # Read start (triggers Range: bytes=0-3)
        assert reader.read_at(0, 4) == raw_data[:4]

        # Read random location (triggers Range: bytes=5000-5009)
        assert reader.read_at(5000, 10) == raw_data[5000:5010]

    finally:
        httpd.shutdown()
        httpd.server_close()


def test_overlay_commit(sample_snapshot, temp_dir):
    base_path, original_data = sample_snapshot
    overlay_path = os.path.join(temp_dir, "overlay.bin")
    final_path = os.path.join(temp_dir, "committed.st")

    # 1. Simulate an overlay file modification
    with open(overlay_path, "wb") as f:
        # Write "M" * 4096 to represent a modified block
        f.write(b"M" * 4096)

    meta_path = os.path.join(temp_dir, "overlay.meta")
    with open(meta_path, "wb") as f:
        # Write block index 0 (u64 little endian) indicating block 0 is modified
        f.write((0).to_bytes(8, byteorder="little"))

    # 2. Commit the overlay
    strata.merge_overlay(base_path, overlay_path, final_path)

    # 3. Verify
    reader = strata.open(final_path)
    # First block should be modified
    assert reader.read_at(0, 1) == b"M"
    # Later blocks should be original
    assert reader.read_at(4096, 1) == original_data[4096:4097]


def test_thin_snapshot(sample_snapshot, temp_dir):
    base_path, _ = sample_snapshot
    overlay_path = os.path.join(temp_dir, "empty_overlay.bin")
    meta_path = os.path.join(temp_dir, "empty_overlay.meta")
    thin_path = os.path.join(temp_dir, "thin.st")

    # Create empty overlay files
    open(overlay_path, "wb").close()
    open(meta_path, "wb").close()

    # Create thin snapshot (references base for unmodified blocks)
    strata.merge_overlay(base_path, overlay_path, thin_path, thin=True)

    # Verify inspection shows parent
    info = strata.inspect(thin_path)
    assert info["parent_path"] == base_path

    # Verify data is readable (transparently resolved)
    reader = strata.open(thin_path)
    assert reader.read_at(0, 10) == strata.open(base_path).read_at(0, 10)


def test_mount(sample_snapshot, temp_dir):
    """
    Tests the strata.mount context manager.
    Requires the 'strata' binary to be built and available.
    """
    snap_path, _ = sample_snapshot

    # Skip if binary not found (e.g. in pure CI environment without cargo build)
    if shutil.which("strata") is None and not os.path.exists("target/release/strata"):
        pytest.skip("strata binary not found")

    try:
        with strata.mount(snap_path) as mp:
            mountpoint = mp.path
            assert os.path.exists(mountpoint)
            assert os.path.exists(os.path.join(mountpoint, "disk"))

            # Read from the mounted file
            with open(os.path.join(mountpoint, "disk"), "rb") as f:
                header = f.read(4)
                # First 4 bytes of our sample data (0, 1, 2, 3)
                assert header == bytes([0, 1, 2, 3])
    except (RuntimeError, TimeoutError) as e:
        pytest.fail(f"Mount failed: {e}")
