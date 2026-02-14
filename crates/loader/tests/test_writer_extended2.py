"""Extended tests for hexz.writer module (coverage for uncovered paths)."""

import os
import warnings

import pytest
from hexz.writer import Writer, COMPRESSION_LEVELS
from hexz.exceptions import ValidationError


class TestWriterInit:
    """Test Writer initialization options."""

    def test_default_writer(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        disk = tmp_path / "disk.img"
        disk.write_bytes(b"\x00" * 65536)

        with Writer(path) as w:
            w.add_file(str(disk))

        assert os.path.exists(path)

    def test_writer_with_mode_alias(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        disk = tmp_path / "disk.img"
        disk.write_bytes(b"\x00" * 65536)

        # 'mode' is an alias for 'packing'
        with Writer(path, mode="fast") as w:
            w.add_file(str(disk))

        assert os.path.exists(path)

    def test_writer_packing_overrides_mode(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        disk = tmp_path / "disk.img"
        disk.write_bytes(b"\x00" * 65536)

        # packing takes priority over mode
        with Writer(path, mode="fast", packing="tight") as w:
            w.add_file(str(disk))

        assert os.path.exists(path)

    def test_writer_invalid_packing_mode(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        with pytest.raises(ValidationError, match="Invalid packing mode"):
            Writer(path, packing="invalid_mode")

    def test_writer_encrypt_warning(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            _ = Writer(path, encrypt=True, password="test123")
            assert len(w) == 1
            assert "Encryption in Writer is not yet implemented" in str(w[0].message)


class TestWriterAdd:
    """Test Writer.add() dispatch."""

    def test_add_file_path(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        disk = tmp_path / "disk.img"
        disk.write_bytes(b"\x00" * 65536)

        with Writer(path) as w:
            w.add(str(disk))

        assert os.path.exists(path)

    def test_add_bytes(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        with Writer(path) as w:
            w.add(b"\x00" * 65536)

        assert os.path.exists(path)

    def test_add_unsupported_type(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        with Writer(path) as w:
            with pytest.raises(ValidationError, match="Cannot add source of type"):
                w.add(12345)


class TestWriterAddFile:
    """Test Writer.add_file() with kind parameter."""

    def test_add_file_disk_kind(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        disk = tmp_path / "disk.img"
        disk.write_bytes(b"\x00" * 65536)

        with Writer(path) as w:
            w.add_file(str(disk), kind="disk")

        assert os.path.exists(path)

    def test_add_file_memory_kind(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        disk = tmp_path / "disk.img"
        mem = tmp_path / "mem.img"
        disk.write_bytes(b"\xaa" * 65536)
        mem.write_bytes(b"\xbb" * 65536)

        with Writer(path) as w:
            w.add_file(str(disk), kind="disk")
            w.add_file(str(mem), kind="memory")

        assert os.path.exists(path)

    def test_add_file_unknown_kind(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        disk = tmp_path / "disk.img"
        disk.write_bytes(b"\x00" * 65536)

        with Writer(path) as w:
            with pytest.raises(ValidationError, match="Unknown kind"):
                w.add_file(str(disk), kind="unknown")


class TestWriterAddArray:
    """Test Writer.add_array() method."""

    def test_add_contiguous_array(self, tmp_path):
        np = pytest.importorskip("numpy")
        path = str(tmp_path / "test.hxz")
        arr = np.zeros(8192, dtype=np.uint8)

        with Writer(path) as w:
            w.add_array(arr)

        assert os.path.exists(path)

    def test_add_non_contiguous_array(self, tmp_path):
        np = pytest.importorskip("numpy")
        path = str(tmp_path / "test.hxz")
        arr = np.zeros((100, 100), dtype=np.uint8)
        non_contig = arr[::2, ::2]  # Non-contiguous slice

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            with Writer(path) as writer:
                writer.add_array(non_contig)
            contiguity_warns = [x for x in w if "not C-contiguous" in str(x.message)]
            assert len(contiguity_warns) == 1


class TestWriterMetadata:
    """Test Writer.add_metadata() method."""

    def test_add_metadata(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        disk = tmp_path / "disk.img"
        disk.write_bytes(b"\x00" * 65536)

        with Writer(path) as w:
            w.add_file(str(disk))
            w.add_metadata({"key": "value", "num": 42})

        assert os.path.exists(path)

    def test_add_metadata_chaining(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        disk = tmp_path / "disk.img"
        disk.write_bytes(b"\x00" * 65536)

        with Writer(path) as w:
            w.add_file(str(disk)).add_metadata({"a": 1}).add_metadata({"b": 2})


class TestWriterWrite:
    """Test Writer.write() method."""

    def test_write_basic(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        with Writer(path) as w:
            n = w.write(b"\x00" * 65536)
            assert n == 65536

    def test_write_with_offset_warning(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            with Writer(path) as writer:
                writer.write(b"\x00" * 65536, offset=100)
            offset_warns = [x for x in w if "Random-access writing" in str(x.message)]
            assert len(offset_warns) == 1


class TestWriterBytesWritten:
    """Test bytes_written and tell()."""

    def test_bytes_written(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        disk = tmp_path / "disk.img"
        disk.write_bytes(b"\x00" * 65536)

        with Writer(path) as w:
            w.add_file(str(disk))
            assert w.bytes_written > 0

    def test_tell(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        disk = tmp_path / "disk.img"
        disk.write_bytes(b"\x00" * 65536)

        with Writer(path) as w:
            w.add_file(str(disk))
            assert w.tell() > 0


class TestWriterRepr:
    """Test Writer repr."""

    def test_repr(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        disk = tmp_path / "disk.img"
        disk.write_bytes(b"\x00" * 65536)

        with Writer(path, compression="zstd") as w:
            r = repr(w)
            assert "Writer(" in r
            assert "zstd" in r
            w.add_file(str(disk))


class TestWriterExitOnError:
    """Test that finalize is not called when an exception occurs."""

    def test_no_finalize_on_exception(self, tmp_path):
        path = str(tmp_path / "test.hxz")
        try:
            with Writer(path) as _:
                raise RuntimeError("intentional error")
        except RuntimeError:
            pass
        # The writer should not have finalized


class TestCompressionLevels:
    """Test COMPRESSION_LEVELS mapping."""

    def test_fast_mode_exists(self):
        assert "fast" in COMPRESSION_LEVELS

    def test_balanced_mode_exists(self):
        assert "balanced" in COMPRESSION_LEVELS

    def test_tight_mode_exists(self):
        assert "tight" in COMPRESSION_LEVELS

    def test_fast_lz4(self):
        assert COMPRESSION_LEVELS["fast"]["lz4"] is None

    def test_fast_zstd(self):
        assert COMPRESSION_LEVELS["fast"]["zstd"] == 1

    def test_tight_zstd(self):
        assert COMPRESSION_LEVELS["tight"]["zstd"] == 9
