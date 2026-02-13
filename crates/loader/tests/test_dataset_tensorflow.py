"""Tests for TFDataset (TensorFlow integration)."""

import pytest
import strata


def test_tfdataset_not_implemented():
    """Test that TFDataset raises NotImplementedError."""
    with pytest.raises(
        NotImplementedError, match="TensorFlow dataset not yet implemented"
    ):
        strata.TFDataset("dummy.st")


def test_tfdataset_as_dataset_not_implemented():
    """Test that TFDataset.as_dataset() raises NotImplementedError."""
    # We need to test the as_dataset method directly by bypassing __init__
    # Create instance without calling __init__
    from strata.dataset import TFDataset

    dataset = TFDataset.__new__(TFDataset)

    # Now call as_dataset which should raise
    with pytest.raises(
        NotImplementedError, match="TensorFlow dataset conversion not yet implemented"
    ):
        dataset.as_dataset()


def test_tfdataset_with_kwargs(tmp_path):
    """Test that TFDataset accepts kwargs but still raises NotImplementedError."""
    # Even with valid-looking arguments, should raise NotImplementedError
    with pytest.raises(NotImplementedError):
        strata.TFDataset(
            "dummy.st",
            item_size=1024,
            cache_size_mb=512,
            shuffle=True,
        )


def test_tfdataset_import_in_module():
    """Test that TFDataset is importable from strata.dataset."""
    from strata.dataset import TFDataset

    assert TFDataset is not None


def test_tfdataset_in_all():
    """Test that TFDataset is in __all__."""
    import strata.dataset

    assert "TFDataset" in strata.dataset.__all__
