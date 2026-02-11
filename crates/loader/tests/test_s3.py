import pytest
import os
import time
import subprocess
import socket
import strata
import shutil

pytest.importorskip("boto3")
import boto3


def get_free_port():
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.bind(("", 0))
    port = s.getsockname()[1]
    s.close()
    return port


@pytest.fixture(scope="module")
def s3_server():
    """
    Spawns a local Moto S3 server.
    """
    if not shutil.which("moto_server"):
        pytest.fail("moto_server not found. Please install: pip install 'moto[server]'")

    port = get_free_port()
    host = "127.0.0.1"
    endpoint_url = f"http://{host}:{port}"

    process = subprocess.Popen(
        ["moto_server", "-p", str(port), "-H", host],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    timeout = 10
    start = time.time()
    connected = False

    while time.time() - start < timeout:
        if process.poll() is not None:
            stdout, stderr = process.communicate()
            pytest.fail(
                f"moto_server exited early.\nSTDOUT: {stdout}\nSTDERR: {stderr}"
            )

        try:
            s = socket.create_connection((host, port), timeout=0.1)
            s.close()
            connected = True
            break
        except (OSError, ConnectionRefusedError):
            time.sleep(0.1)

    if not connected:
        process.kill()
        stdout, stderr = process.communicate()
        pytest.fail(
            f"moto_server failed to start within {timeout}s.\nSTDOUT: {stdout}\nSTDERR: {stderr}"
        )

    yield endpoint_url

    process.terminate()
    process.wait()


@pytest.fixture
def s3_client(s3_server):
    os.environ["AWS_ACCESS_KEY_ID"] = "testing"
    os.environ["AWS_SECRET_ACCESS_KEY"] = "testing"
    os.environ["AWS_REGION"] = "us-east-1"

    return boto3.client("s3", endpoint_url=s3_server, region_name="us-east-1")


def test_async_s3_read(s3_server, s3_client, tmp_path):
    """
    Tests that the Rust AsyncS3Backend can read from a custom S3 endpoint.
    """
    bucket = "my-test-bucket"
    key = "snapshot.st"

    # 1. Create a dummy snapshot file locally
    # We use a distinct pattern to verify we aren't reading zeros
    data = bytes([i % 255 for i in range(1024 * 10)])  # 10KB data

    local_snap = str(tmp_path / "snap.st")
    local_data = str(tmp_path / "data.bin")

    with open(local_data, "wb") as f:
        f.write(data)

    with strata.open(local_snap, mode="w") as w:
        w.add(local_data)

    # Read the generated snapshot bytes (the .st file) to upload to S3
    with open(local_snap, "rb") as f:
        snap_file_bytes = f.read()

    # 2. Upload snapshot to Mock S3
    s3_client.create_bucket(Bucket=bucket)
    s3_client.put_object(Bucket=bucket, Key=key, Body=snap_file_bytes)

    # 3. Open via Strata using the s3:// URI and custom endpoint
    uri = f"s3://{bucket}/{key}"

    print(f"Connecting to {uri} at {s3_server}")
    reader = strata.open(uri, endpoint_url=s3_server, allow_restricted=True)

    # 4. Verify Reads against ORIGINAL DATA
    # The reader returns logical data (uncompressed), so we compare against `data`
    assert reader.size == len(data)

    # Read header from logical disk
    header = reader.read(4, offset=0)
    assert header == data[:4]

    # Read middle from logical disk
    mid = len(data) // 2
    chunk = reader.read(100, offset=mid)
    assert chunk == data[mid : mid + 100]
