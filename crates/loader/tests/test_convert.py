"""Tests for hexz.convert — data format conversion utilities."""

import io
import os
import tarfile

import pytest

import hexz
from hexz.convert import _detect_format, convert


# ═══════════════════════════════════════════════════════════════════════════════
#  Fixtures
# ═══════════════════════════════════════════════════════════════════════════════


@pytest.fixture
def out_path(test_dir):
    """Generate a unique output snapshot path."""
    return os.path.join(test_dir, "convert_output.hxz")


def _make_tar(path, files, *, compress=None):
    """Create a tar archive with the given files.

    Args:
        path: Output tar path.
        files: Dict of {name: bytes_content}.
        compress: Optional compression ('gz', 'bz2', 'xz').
    """
    mode = "w"
    if compress:
        mode = f"w:{compress}"

    with tarfile.open(path, mode) as tf:
        for name, data in files.items():
            info = tarfile.TarInfo(name=name)
            info.size = len(data)
            tf.addfile(info, io.BytesIO(data))


def _make_webdataset(path, samples):
    """Create a WebDataset-style tar archive.

    Args:
        path: Output tar path.
        samples: Dict of {sample_key: {ext: bytes_content}}.
            e.g. {"000": {".jpg": b"...", ".cls": b"..."}}
    """
    with tarfile.open(path, "w") as tf:
        for key, extensions in samples.items():
            for ext, data in extensions.items():
                name = f"{key}{ext}"
                info = tarfile.TarInfo(name=name)
                info.size = len(data)
                tf.addfile(info, io.BytesIO(data))


# ═══════════════════════════════════════════════════════════════════════════════
#  Format auto-detection
# ═══════════════════════════════════════════════════════════════════════════════


class TestAutoDetect:
    def test_tar(self):
        assert _detect_format("data.tar") == "tar"

    def test_tar_gz(self):
        assert _detect_format("data.tar.gz") == "tar"

    def test_tgz(self):
        assert _detect_format("archive.tgz") == "tar"

    def test_tar_bz2(self):
        assert _detect_format("data.tar.bz2") == "tar"

    def test_tar_xz(self):
        assert _detect_format("data.tar.xz") == "tar"

    def test_h5(self):
        assert _detect_format("features.h5") == "hdf5"

    def test_hdf5(self):
        assert _detect_format("features.hdf5") == "hdf5"

    def test_webdataset(self):
        assert _detect_format("dataset.wds") == "webdataset"

    def test_unknown_raises(self):
        with pytest.raises(hexz.ValidationError):
            _detect_format("data.csv")

    def test_no_extension_raises(self):
        with pytest.raises(hexz.ValidationError):
            _detect_format("noext")


# ═══════════════════════════════════════════════════════════════════════════════
#  Tar conversion
# ═══════════════════════════════════════════════════════════════════════════════


class TestConvertTar:
    def test_basic(self, test_dir, out_path):
        tar_path = os.path.join(test_dir, "basic.tar")
        _make_tar(
            tar_path,
            {
                "hello.txt": b"Hello, world!",
                "data.bin": os.urandom(4096),
            },
        )

        meta = convert(tar_path, out_path)

        assert os.path.isfile(out_path)
        assert meta.primary_size > 0
        assert meta["source"]["format"] == "tar"
        assert meta["source"]["total_files"] == 2

    def test_tar_gz(self, test_dir):
        tar_path = os.path.join(test_dir, "compressed.tar.gz")
        out = os.path.join(test_dir, "from_tgz.hxz")
        _make_tar(
            tar_path,
            {
                "file1.txt": b"content one",
                "file2.txt": b"content two",
            },
            compress="gz",
        )

        meta = convert(tar_path, out)

        assert os.path.isfile(out)
        assert meta["source"]["format"] == "tar"
        assert meta["source"]["total_files"] == 2

    def test_metadata_manifest(self, test_dir):
        tar_path = os.path.join(test_dir, "manifest.tar")
        out = os.path.join(test_dir, "manifest.hxz")
        files = {
            "a.txt": b"aaa",
            "subdir/b.bin": b"bbbbb",
            "c.dat": b"c" * 100,
        }
        _make_tar(tar_path, files)

        meta = convert(tar_path, out)

        source = meta["source"]
        assert source["total_files"] == 3
        assert source["total_bytes"] == 3 + 5 + 100

        # Verify individual file entries
        names = [f["name"] for f in source["source_files"]]
        assert "a.txt" in names
        assert "subdir/b.bin" in names
        assert "c.dat" in names

        # Verify sizes match
        for entry in source["source_files"]:
            assert entry["size"] == len(files[entry["name"]])

    def test_empty_tar(self, test_dir):
        tar_path = os.path.join(test_dir, "empty.tar")
        out = os.path.join(test_dir, "from_empty.hxz")
        _make_tar(tar_path, {})

        meta = convert(tar_path, out)

        assert os.path.isfile(out)
        assert meta["source"]["total_files"] == 0

    def test_roundtrip_data_integrity(self, test_dir):
        """Verify that data written through convert is readable."""
        tar_path = os.path.join(test_dir, "integrity.tar")
        content = os.urandom(8192)
        _make_tar(tar_path, {"data.bin": content})

        out = os.path.join(test_dir, "integrity.hxz")
        convert(tar_path, out)

        with hexz.open(out) as reader:
            read_data = reader.read(len(content))
            assert read_data == content


# ═══════════════════════════════════════════════════════════════════════════════
#  HDF5 conversion (skipped if h5py not available)
# ═══════════════════════════════════════════════════════════════════════════════

try:
    import h5py
    import numpy as np

    _HAS_H5PY = True
except ImportError:
    _HAS_H5PY = False


@pytest.mark.skipif(not _HAS_H5PY, reason="h5py not installed")
class TestConvertHDF5:
    def test_basic(self, test_dir):
        h5_path = os.path.join(test_dir, "basic.h5")
        out = os.path.join(test_dir, "from_h5.hxz")

        with h5py.File(h5_path, "w") as f:
            f.create_dataset(
                "features", data=np.random.randn(100, 64).astype(np.float32)
            )
            f.create_dataset("labels", data=np.arange(100, dtype=np.int64))

        meta = convert(h5_path, out)

        assert os.path.isfile(out)
        assert meta["source"]["format"] == "hdf5"
        assert meta["source"]["total_datasets"] == 2

    def test_metadata_datasets(self, test_dir):
        h5_path = os.path.join(test_dir, "meta.h5")
        out = os.path.join(test_dir, "meta_h5.hxz")

        with h5py.File(h5_path, "w") as f:
            f.create_dataset("group/data", data=np.zeros((10, 20), dtype=np.float64))

        meta = convert(h5_path, out)

        datasets = meta["source"]["datasets"]
        assert len(datasets) == 1
        assert datasets[0]["path"] == "group/data"
        assert datasets[0]["shape"] == [10, 20]
        assert "float64" in datasets[0]["dtype"]

    def test_import_error_without_h5py(self, test_dir, monkeypatch):
        """Verify helpful error when h5py is not installed."""

        h5_path = os.path.join(test_dir, "fake.h5")
        out = os.path.join(test_dir, "noh5py.hxz")

        # Create a minimal fake file so FileNotFoundError isn't raised first
        with open(h5_path, "wb") as f:
            f.write(b"\x89HDF")

        # Temporarily make h5py unimportable
        import builtins

        real_import = builtins.__import__

        def mock_import(name, *args, **kwargs):
            if name == "h5py":
                raise ImportError("No module named 'h5py'")
            return real_import(name, *args, **kwargs)

        monkeypatch.setattr(builtins, "__import__", mock_import)

        with pytest.raises(ImportError, match="h5py is required"):
            convert(h5_path, out, format="hdf5")


# ═══════════════════════════════════════════════════════════════════════════════
#  WebDataset conversion
# ═══════════════════════════════════════════════════════════════════════════════


class TestConvertWebDataset:
    def test_basic(self, test_dir):
        wds_path = os.path.join(test_dir, "basic.wds")
        out = os.path.join(test_dir, "from_wds.hxz")

        _make_webdataset(
            wds_path,
            {
                "000": {".jpg": b"fake_image_data", ".cls": b"0"},
                "001": {".jpg": b"another_image", ".cls": b"1"},
            },
        )

        meta = convert(wds_path, out)

        assert os.path.isfile(out)
        assert meta["source"]["format"] == "webdataset"
        assert meta["source"]["total_samples"] == 2
        assert meta["source"]["total_files"] == 4

    def test_sample_grouping(self, test_dir):
        wds_path = os.path.join(test_dir, "grouped.wds")
        out = os.path.join(test_dir, "grouped_wds.hxz")

        _make_webdataset(
            wds_path,
            {
                "sample_a": {
                    ".png": b"png_data",
                    ".json": b'{"label": 0}',
                    ".txt": b"caption",
                },
                "sample_b": {".png": b"other_png"},
            },
        )

        meta = convert(wds_path, out)

        samples = meta["source"]["samples"]
        assert "sample_a" in samples
        assert "sample_b" in samples
        assert len(samples["sample_a"]) == 3
        assert len(samples["sample_b"]) == 1


# ═══════════════════════════════════════════════════════════════════════════════
#  General convert() behavior
# ═══════════════════════════════════════════════════════════════════════════════


class TestConvertGeneral:
    def test_explicit_format_overrides_extension(self, test_dir):
        """A .bin file should work when format='tar' is explicit."""
        tar_path = os.path.join(test_dir, "explicit.bin")
        out = os.path.join(test_dir, "explicit.hxz")
        _make_tar(tar_path, {"x.txt": b"data"})

        meta = convert(tar_path, out, format="tar")

        assert meta["source"]["format"] == "tar"

    def test_unknown_format_raises(self, test_dir):
        tar_path = os.path.join(test_dir, "dummy_for_fmt.tar")
        out = os.path.join(test_dir, "unknown_fmt.hxz")
        _make_tar(tar_path, {"x.txt": b"data"})

        with pytest.raises(hexz.ValidationError, match="Unknown format"):
            convert(tar_path, out, format="parquet")

    def test_nonexistent_input_raises(self, test_dir):
        out = os.path.join(test_dir, "nonexist.hxz")

        with pytest.raises(FileNotFoundError):
            convert("/nonexistent/file.tar", out)

    def test_with_profile(self, test_dir):
        tar_path = os.path.join(test_dir, "profiled.tar")
        out = os.path.join(test_dir, "profiled.hxz")
        _make_tar(tar_path, {"data.bin": os.urandom(2048)})

        meta = convert(tar_path, out, profile="ml")

        assert os.path.isfile(out)
        assert meta.primary_size > 0

    def test_with_zstd_compression(self, test_dir):
        tar_path = os.path.join(test_dir, "zstd_conv.tar")
        out = os.path.join(test_dir, "zstd_conv.hxz")
        _make_tar(tar_path, {"file.txt": b"compressible " * 500})

        meta = convert(tar_path, out, compression="zstd")

        assert os.path.isfile(out)
        assert meta.compression.lower() == "zstd"

    def test_unknown_profile_raises(self, test_dir):
        tar_path = os.path.join(test_dir, "bad_profile.tar")
        out = os.path.join(test_dir, "bad_profile.hxz")
        _make_tar(tar_path, {"x.txt": b"data"})

        with pytest.raises(hexz.ValidationError, match="Unknown profile"):
            convert(tar_path, out, profile="nonexistent")
