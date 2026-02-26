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
    """CDC is always-on; verify dedup handles shifted data."""
    chunk = os.urandom(1024 * 1024)  # 1MB random
    data_shifted = chunk + b"insertion" + chunk

    path = os.path.join(test_dir, "cdc_dedup.hxz")
    with hexz.Writer(path, dedup=True, compression="lz4") as w:
        w.add_bytes(data_shifted)
        size = w.bytes_written

    # CDC + dedup should recognize the repeated chunk despite the insertion
    assert size < len(data_shifted)
