import pytest
import os
import shutil
import tempfile
import threading
import http.server
import socketserver
import strata
import numpy as np


@pytest.fixture(scope="session")
def test_dir():
    """Create a temporary directory for the test session."""
    tmp_dir = tempfile.mkdtemp(prefix="strata_test_")
    yield tmp_dir
    shutil.rmtree(tmp_dir)


@pytest.fixture(scope="session")
def raw_data_path(test_dir):
    """Generate a raw random file."""
    path = os.path.join(test_dir, "test_source.raw")
    size = 1024 * 1024  # 1MB
    with open(path, "wb") as f:
        f.write(os.urandom(size))
    return path


@pytest.fixture(scope="session")
def base_snap_path(test_dir, raw_data_path):
    """Create a base snapshot."""
    snap_path = os.path.join(test_dir, "test_base.st")
    with strata.SnapshotBuilder(snap_path, compression="lz4") as builder:
        builder.add_disk(raw_data_path)
    return snap_path


@pytest.fixture(scope="session")
def http_server(base_snap_path):
    """Start a simple HTTP server serving the snapshot."""

    class RangeRequestHandler(http.server.BaseHTTPRequestHandler):
        def log_message(self, format, *args):
            pass

        def do_HEAD(self):
            if self.path.endswith(os.path.basename(base_snap_path)):
                size = os.path.getsize(base_snap_path)
                self.send_response(200)
                self.send_header("Content-Length", str(size))
                self.send_header("Accept-Ranges", "bytes")
                self.end_headers()
            else:
                self.send_error(404)

        def do_GET(self):
            if not self.path.endswith(os.path.basename(base_snap_path)):
                self.send_error(404)
                return

            size = os.path.getsize(base_snap_path)
            range_header = self.headers.get("Range")

            if range_header:
                try:
                    _, r = range_header.split("=")
                    start_str, end_str = r.split("-")
                    start = int(start_str)
                    end = int(end_str) if end_str else size - 1
                    end = min(end, size - 1)
                    length = end - start + 1

                    self.send_response(206)
                    self.send_header("Content-Range", f"bytes {start}-{end}/{size}")
                    self.send_header("Content-Length", str(length))
                    self.send_header("Content-Type", "application/octet-stream")
                    self.end_headers()

                    with open(base_snap_path, "rb") as f:
                        f.seek(start)
                        self.wfile.write(f.read(length))
                    return
                except Exception:
                    pass

            self.send_response(200)
            self.send_header("Content-Length", str(size))
            self.end_headers()
            with open(base_snap_path, "rb") as f:
                shutil.copyfileobj(f, self.wfile)

    # Use port 0 to let the OS choose a free port
    server = socketserver.TCPServer(("localhost", 0), RangeRequestHandler)
    port = server.server_address[1]

    thread = threading.Thread(target=server.serve_forever)
    thread.daemon = True
    thread.start()

    yield f"http://localhost:{port}/{os.path.basename(base_snap_path)}"

    server.shutdown()
    server.server_close()
