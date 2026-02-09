import pytest
from strata import open, AsyncStrataReader

# List of URLs that must be blocked by the SSRF protection
RESTRICTED_URLS = [
    # Loopback
    "http://127.0.0.1/snapshot.st",
    "http://[::1]/snapshot.st",
    "http://localhost/snapshot.st",
    # Cloud Metadata (AWS/GCP/Azure)
    "http://169.254.169.254/latest/meta-data",
    # Private Networks
    "http://10.0.0.1/snapshot.st",
    "http://192.168.1.1/snapshot.st",
    "http://172.16.0.1/snapshot.st",
    # IPv6 Unique Local
    "http://[fc00::1]/snapshot.st",
]


@pytest.mark.parametrize("url", RESTRICTED_URLS)
def test_ssrf_sync_blocked(url):
    """
    Verify that the synchronous StrataReader rejects internal/private IPs.
    """
    with pytest.raises(OSError) as excinfo:
        # This calls StrataReader(url) internally in Rust
        open(url)

    error_msg = str(excinfo.value)
    # Ensure the rejection comes from our security check, not a connection error
    assert "Access to internal/private IP denied" in error_msg


@pytest.mark.asyncio
@pytest.mark.parametrize("url", RESTRICTED_URLS)
async def test_ssrf_async_blocked(url):
    """
    Verify that the AsyncStrataReader rejects internal/private IPs.
    """
    with pytest.raises(OSError) as excinfo:
        await AsyncStrataReader.create(url)

    error_msg = str(excinfo.value)
    assert "Access to internal/private IP denied" in error_msg


def test_ssrf_public_allowed():
    """
    Verify that a public URL passes the SSRF check.

    It will likely fail later (connection error, 404, or invalid header),
    but the error MUST NOT be the SSRF denial message.
    """
    # example.com resolves to a public IP (e.g., 93.184.216.34)
    url = "http://example.com/snapshot.st"

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
