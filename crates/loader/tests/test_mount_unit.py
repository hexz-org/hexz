"""Unit tests for mount.py that don't require the strata binary or FUSE."""

import os
import pytest
from unittest.mock import patch
from strata.mount import _MountPoint, mount


class TestMountFunction:
    """Test the mount() factory function."""

    def test_returns_mountpoint(self):
        mp = mount("test.st")
        assert isinstance(mp, _MountPoint)

    def test_path_is_absolute(self):
        mp = mount("test.st")
        assert os.path.isabs(mp.snapshot_path)

    def test_mount_with_mount_point(self, tmp_path):
        mp = mount("test.st", mount_point=str(tmp_path / "mnt"))
        assert mp.mount_point is not None
        assert os.path.isabs(mp.mount_point)

    def test_mount_with_none_mount_point(self):
        mp = mount("test.st")
        assert mp.mount_point is None

    def test_mount_with_custom_binary(self):
        mp = mount("test.st", binary="/usr/local/bin/strata")
        assert mp.binary == "/usr/local/bin/strata"


class TestMountPointInit:
    """Test _MountPoint initialization."""

    def test_default_binary(self):
        mp = _MountPoint("/tmp/test.st")
        assert mp.binary == "strata"

    def test_custom_binary(self):
        mp = _MountPoint("/tmp/test.st", binary="/custom/strata")
        assert mp.binary == "/custom/strata"

    def test_mount_point_none(self):
        mp = _MountPoint("/tmp/test.st")
        assert mp.mount_point is None
        assert mp._temp_dir is None
        assert mp._process is None

    def test_mount_point_absolute(self, tmp_path):
        mp = _MountPoint("/tmp/test.st", mount_point=str(tmp_path))
        assert os.path.isabs(mp.mount_point)

    def test_path_property_before_mount(self):
        mp = _MountPoint("/tmp/test.st")
        assert mp.path is None

    def test_path_property_with_mount_point(self, tmp_path):
        mp = _MountPoint("/tmp/test.st", mount_point=str(tmp_path))
        assert mp.path == str(tmp_path)


class TestFindBinary:
    """Test _find_binary method."""

    @patch("shutil.which", return_value=None)
    @patch("os.path.exists", return_value=False)
    def test_binary_not_found(self, mock_exists, mock_which):
        mp = _MountPoint("/tmp/test.st", binary="nonexistent_xyz")
        with pytest.raises(FileNotFoundError, match="Could not find"):
            mp._find_binary()

    @patch("shutil.which", return_value="/usr/bin/strata")
    def test_binary_found_in_path(self, mock_which):
        mp = _MountPoint("/tmp/test.st")
        result = mp._find_binary()
        assert result == "strata"


class TestMountPointExit:
    """Test __exit__ cleanup."""

    def test_exit_with_no_process(self):
        mp = _MountPoint("/tmp/test.st")
        mp.mount_point = "/tmp/test_mount"
        mp._process = None
        # Should not raise
        mp.__exit__(None, None, None)

    def test_exit_with_no_mount_point(self):
        mp = _MountPoint("/tmp/test.st")
        mp.mount_point = None
        mp._process = None
        # Should not raise
        mp.__exit__(None, None, None)


class TestMountAll:
    """Test __all__ export."""

    def test_all_contains_mount(self):
        from strata import mount as mount_mod

        assert hasattr(mount_mod, "__all__") is False or "mount" in getattr(
            mount_mod, "__all__", []
        )
