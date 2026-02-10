import strata
import os
import json
import pytest


def test_writer_metadata(test_dir):
    path = os.path.join(test_dir, "metadata_test.st")

    metadata = {"author": "gemini", "dataset_id": 12345, "tags": ["test", "metadata"]}

    with strata.Writer(path) as w:
        w.add_bytes(b"some data")
        w.add_metadata(metadata)

    # verify with inspect
    meta = strata.inspect(path)
    assert meta["author"] == "gemini"
    assert meta["dataset_id"] == 12345
    assert meta["tags"] == ["test", "metadata"]

    # verify with reader
    with strata.open(path) as r:
        assert r.metadata["author"] == "gemini"


def test_writer_bytes_written(test_dir):
    path = os.path.join(test_dir, "bytes_test.st")
    data = b"hello world" * 100

    with strata.Writer(path) as w:
        initial = w.bytes_written
        assert initial > 0  # Header is written on init
        w.add_bytes(data)
        # Bytes written should increase. Note: it might be compressed size + overhead
        # or uncompressed size depending on implementation.
        # StrataBuilder::current_offset tracks file offset.
        assert w.bytes_written > initial

    final_size = os.path.getsize(path)
    # bytes_written should match file size approx (offset includes headers)
    # The builder.get_bytes_written() returns current_offset which is file size.

    # Re-open to check size
    w = strata.Writer(path)
    # It starts at header size
    assert w.bytes_written > 0


def test_writer_dedup_cdc(test_dir):
    path = os.path.join(test_dir, "dedup_test.st")

    # Create data with repetition
    chunk = os.urandom(1024 * 1024)  # 1MB random
    data = chunk * 4  # 4MB total

    # Write with CDC
    with strata.Writer(path, dedup=True, cdc=True, compression="lz4") as w:
        w.add_bytes(data)
        size_dedup = w.bytes_written

    # Write without CDC (fixed block dedup might catch it if aligned, but let's see)
    # Actually, if we use same chunk 4 times, fixed block dedup should catch it too.
    # To test CDC specifically, we need shift.

    data_shifted = chunk + b"insertion" + chunk

    path_cdc = os.path.join(test_dir, "cdc.st")
    with strata.Writer(path_cdc, dedup=True, cdc=True) as w:
        w.add_bytes(data_shifted)
        size_cdc = w.bytes_written

    path_nocdc = os.path.join(test_dir, "nocdc.st")
    with strata.Writer(path_nocdc, dedup=True, cdc=False) as w:
        w.add_bytes(data_shifted)
        size_nocdc = w.bytes_written

    # CDC should handle insertion better than fixed blocks
    # fixed blocks will likely fail to dedup the second chunk because of shift
    assert size_cdc < size_nocdc


def test_writer_cdc_param(test_dir):
    # Just verify we can pass the parameter
    path = os.path.join(test_dir, "param_test.st")
    with strata.Writer(path, cdc=True) as w:
        w.add_bytes(b"test")
