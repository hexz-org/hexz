"""Extended tests for AsyncReader to improve reader.py coverage."""

import pytest
from strata.reader import AsyncReader


class TestAsyncReaderInit:
    """Test AsyncReader initialization."""

    def test_repr(self):
        reader = AsyncReader("test.st")
        assert "AsyncReader" in repr(reader)
        assert "test.st" in repr(reader)

    def test_ensure_open_raises(self):
        reader = AsyncReader("test.st")
        with pytest.raises(RuntimeError, match="must be used as async with"):
            reader._ensure_open()

    def test_size_without_open_raises(self):
        reader = AsyncReader("test.st")
        with pytest.raises(RuntimeError):
            reader.size()

    def test_tell_without_open_raises(self):
        reader = AsyncReader("test.st")
        with pytest.raises(RuntimeError):
            reader.tell()

    def test_init_with_options(self):
        reader = AsyncReader(
            "test.st",
            cache_size="512M",
            prefetch=False,
            s3_region="us-east-1",
            endpoint_url="http://localhost:9000",
            allow_restricted=True,
        )
        assert reader._path == "test.st"
        assert reader._cache_capacity_bytes == 512 * 1024 * 1024
        assert reader._prefetch_count == 0
        assert reader._s3_region == "us-east-1"
        assert reader._endpoint_url == "http://localhost:9000"
        assert reader._allow_restricted is True

    def test_init_default_options(self):
        reader = AsyncReader("test.st")
        assert reader._prefetch_count == 4
        assert reader._cache_capacity_bytes is None
        assert reader._s3_region is None
        assert reader._endpoint_url is None
        assert reader._allow_restricted is False
        assert reader._reader is None


@pytest.mark.asyncio
class TestAsyncReaderAsync:
    """Test AsyncReader async methods that don't require file access."""

    async def test_read_without_open_raises(self):
        reader = AsyncReader("test.st")
        with pytest.raises(RuntimeError, match="must be used as async with"):
            await reader.read(100)

    async def test_seek_without_open_raises(self):
        reader = AsyncReader("test.st")
        with pytest.raises(RuntimeError, match="must be used as async with"):
            await reader.seek(0)
