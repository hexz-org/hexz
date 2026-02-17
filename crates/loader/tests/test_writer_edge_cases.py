"""
Tests for edge cases and error handling in hexz.Writer.

This module tests the uncovered edge cases in writer.py:
- Invalid packing mode
- Encryption warnings
- add() method with numpy arrays
- add_file() with kind="memory"
- tell() method
- __repr__ method
"""

import pytest
import tempfile
import numpy as np
import hexz


def test_writer_invalid_packing_mode():
    """Test ValidationError on invalid packing mode."""
    with tempfile.NamedTemporaryFile(suffix=".hxz") as tmp:
        with pytest.raises(hexz.ValidationError, match="Invalid packing mode"):
            hexz.Writer(tmp.name, packing="invalid_mode")


def test_writer_encryption_warning():
    """Test warning when encryption is requested."""
    with tempfile.NamedTemporaryFile(suffix=".hxz") as tmp:
        with pytest.warns(
            UserWarning, match="Encryption in Writer is not yet implemented"
        ):
            writer = hexz.Writer(tmp.name, encrypt=True, password="secret")
            writer.finalize()


def test_writer_add_numpy_array():
    """Test add() method with numpy array dispatches to add_array()."""
    with tempfile.NamedTemporaryFile(suffix=".hxz") as tmp:
        arr = np.arange(100, dtype=np.float32)
        with hexz.Writer(tmp.name) as w:
            w.add(arr)  # Should dispatch to add_array()

        # Verify the array was written
        reader = hexz.Reader(tmp.name)
        data = reader.read(arr.nbytes)
        assert len(data) == arr.nbytes


def test_writer_add_file_memory_kind():
    """Test add_file() with kind='memory'."""
    with tempfile.NamedTemporaryFile(suffix=".img", delete=False) as input_file:
        input_file.write(b"test data for secondary stream")
        input_file.flush()
        input_path = input_file.name

    try:
        with tempfile.NamedTemporaryFile(suffix=".hxz") as tmp:
            with hexz.Writer(tmp.name) as w:
                w.add_file(input_path, kind="memory")

            # Verify file was written
            reader = hexz.Reader(tmp.name)
            # Secondary stream should have data
            assert reader.metadata.secondary_size > 0
    finally:
        import os

        os.unlink(input_path)


def test_writer_tell_method():
    """Test tell() returns current byte position."""
    with tempfile.NamedTemporaryFile(suffix=".hxz") as tmp:
        with hexz.Writer(tmp.name) as w:
            # Initial position
            pos1 = w.tell()
            assert pos1 >= 0

            # Write some data
            w.add_bytes(b"test data")

            # Position should have advanced
            pos2 = w.tell()
            assert pos2 > pos1


def test_writer_repr():
    """Test __repr__ output."""
    with tempfile.NamedTemporaryFile(suffix=".hxz") as tmp:
        writer = hexz.Writer(tmp.name, compression="zstd", block_size=131072)
        repr_str = repr(writer)

        # Should contain key information
        assert "Writer" in repr_str
        assert tmp.name in repr_str
        assert "zstd" in repr_str

        writer.finalize()


def test_writer_add_method_with_bytes():
    """Test add() method with bytes."""
    with tempfile.NamedTemporaryFile(suffix=".hxz") as tmp:
        with hexz.Writer(tmp.name) as w:
            w.add(b"test bytes data")

        reader = hexz.Reader(tmp.name)
        data = reader.read(15)
        assert data == b"test bytes data"


def test_writer_add_array_non_contiguous():
    """Test add_array() with non-contiguous arrays."""
    with tempfile.NamedTemporaryFile(suffix=".hxz") as tmp:
        # Create a non-contiguous array (sliced)
        arr = np.arange(100, dtype=np.float32).reshape(10, 10)
        non_contiguous = arr[:, ::2]  # Skip every other column
        assert not non_contiguous.flags["C_CONTIGUOUS"]

        with hexz.Writer(tmp.name) as w:
            # add_array should handle this by making it contiguous
            w.add_array(non_contiguous)

        reader = hexz.Reader(tmp.name)
        # Should have written the contiguous version
        assert reader.metadata.primary_size > 0


def test_writer_add_metadata():
    """Test add_metadata() method."""
    with tempfile.NamedTemporaryFile(suffix=".hxz") as tmp:
        metadata = {"key": "value", "number": 42}
        with hexz.Writer(tmp.name) as w:
            w.add_metadata(metadata)
            w.add_bytes(b"some data")

        # Metadata should be stored
        reader = hexz.Reader(tmp.name)
        assert reader.metadata.primary_size > 0


def test_writer_finalize_multiple_times():
    """Test that finalize() can be called multiple times safely."""
    with tempfile.NamedTemporaryFile(suffix=".hxz") as tmp:
        with hexz.Writer(tmp.name) as w:
            w.add_bytes(b"test")
            w.finalize()
            # Second finalize should be safe (idempotent)
            w.finalize()


def test_writer_add_bytes_error_handling():
    """Test add_bytes() error handling and cleanup."""
    with tempfile.NamedTemporaryFile(suffix=".hxz") as tmp:
        with hexz.Writer(tmp.name) as w:
            # Valid write
            w.add_bytes(b"test data")

            # Try to write invalid type (should raise error)
            with pytest.raises((TypeError, AttributeError)):
                w.add_bytes("not bytes")  # String instead of bytes


def test_writer_context_manager_cleanup():
    """Test that context manager properly cleans up on error."""
    with tempfile.NamedTemporaryFile(suffix=".hxz") as tmp:
        try:
            with hexz.Writer(tmp.name) as w:
                w.add_bytes(b"data")
                # Force an error
                raise RuntimeError("Test error")
        except RuntimeError:
            pass

        # File should still be created and valid
        import os

        assert os.path.exists(tmp.name)
