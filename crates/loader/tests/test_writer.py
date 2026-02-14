import hexz
import os


def test_writer_metadata(test_dir):
    path = os.path.join(test_dir, "metadata_test.hxz")

    metadata = {"author": "gemini", "dataset_id": 12345, "tags": ["test", "metadata"]}

    with hexz.Writer(path) as w:
        w.add_bytes(b"some data")
        w.add_metadata(metadata)

    # verify with inspect
    meta = hexz.inspect(path)
    assert meta["author"] == "gemini"
    assert meta["dataset_id"] == 12345
    assert meta["tags"] == ["test", "metadata"]

    # verify with reader
    with hexz.open(path) as r:
        assert r.metadata["author"] == "gemini"


def test_writer_bytes_written(test_dir):
    path = os.path.join(test_dir, "bytes_test.hxz")
    data = b"hello world" * 100

    with hexz.Writer(path) as w:
        initial = w.bytes_written
        assert initial > 0  # Header is written on init
        w.add_bytes(data)
        # Bytes written should increase. Note: it might be compressed size + overhead
        # or uncompressed size depending on implementation.
        # Builder::current_offset tracks file offset.
        assert w.bytes_written > initial

    os.path.getsize(path)
    # bytes_written should match file size approx (offset includes headers)
    # The builder.get_bytes_written() returns current_offset which is file size.

    # Re-open to check size
    w = hexz.Writer(path)
    # It starts at header size
    assert w.bytes_written > 0


def test_writer_dedup_cdc(test_dir):
    path = os.path.join(test_dir, "dedup_test.hxz")

    # Create data with repetition
    chunk = os.urandom(1024 * 1024)  # 1MB random
    data = chunk * 4  # 4MB total

    # Write with CDC
    with hexz.Writer(path, dedup=True, cdc=True, compression="lz4") as w:
        w.add_bytes(data)

    # Write without CDC (fixed block dedup might catch it if aligned, but let's see)
    # Actually, if we use same chunk 4 times, fixed block dedup should catch it too.
    # To test CDC specifically, we need shift.

    data_shifted = chunk + b"insertion" + chunk

    path_cdc = os.path.join(test_dir, "cdc.hxz")
    with hexz.Writer(path_cdc, dedup=True, cdc=True) as w:
        w.add_bytes(data_shifted)
        size_cdc = w.bytes_written

    path_nocdc = os.path.join(test_dir, "nocdc.hxz")
    with hexz.Writer(path_nocdc, dedup=True, cdc=False) as w:
        w.add_bytes(data_shifted)
        size_nocdc = w.bytes_written

    # CDC should handle insertion better than fixed blocks
    # fixed blocks will likely fail to dedup the second chunk because of shift
    assert size_cdc < size_nocdc


def test_writer_cdc_param(test_dir):
    # Just verify we can pass the parameter
    path = os.path.join(test_dir, "param_test.hxz")
    with hexz.Writer(path, cdc=True) as w:
        w.add_bytes(b"test")
