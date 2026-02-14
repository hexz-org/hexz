import pytest
from hexz import open, AsyncReader

# List of URLs that must be blocked by the SSRF protection
RESTRICTED_URLS = [
    # Loopback
    "http://127.0.0.1/snapshot.hxz",
    "http://[::1]/snapshot.hxz",
    "http://localhost/snapshot.hxz",
    # Cloud Metadata (AWS/GCP/Azure)
    "http://169.254.169.254/latest/meta-data",
    # Private Networks
    "http://10.0.0.1/snapshot.hxz",
    "http://192.168.1.1/snapshot.hxz",
    "http://172.16.0.1/snapshot.hxz",
    # IPv6 Unique Local
    "http://[fc00::1]/snapshot.hxz",
]


@pytest.mark.parametrize("url", RESTRICTED_URLS)
def test_ssrf_sync_blocked(url):
    """
    Verify that the synchronous Reader rejects internal/private IPs.
    """
    with pytest.raises(OSError) as excinfo:
        # This calls Reader(url) internally in Rust
        open(url)

    error_msg = str(excinfo.value)
    # Ensure the rejection comes from our security check, not a connection error
    assert "Access to internal/private IP denied" in error_msg


@pytest.mark.asyncio
@pytest.mark.parametrize("url", RESTRICTED_URLS)
async def test_ssrf_async_blocked(url):
    """
    Verify that the AsyncReader rejects internal/private IPs.
    """
    with pytest.raises(OSError) as excinfo:
        async with AsyncReader(url) as reader:
            await reader.read(1)

    error_msg = str(excinfo.value)
    assert "Access to internal/private IP denied" in error_msg


def test_ssrf_public_allowed():
    """
    Verify that a public URL passes the SSRF check.

    It will likely fail later (connection error, 404, or invalid header),
    but the error MUST NOT be the SSRF denial message.
    """
    # example.com resolves to a public IP (e.g., 93.184.216.34)
    url = "http://example.com/snapshot.hxz"

    try:
        open(url)
    except OSError as e:
        error_msg = str(e)
        # If the error is about permissions, our check failed.
        # If it's about connection/headers, our check passed.
        assert "Access to internal/private IP denied" not in error_msg
    except Exception:
        # Any other exception means the SSRF check passed
        pass
