"""Unit tests for mount.py that don't require the hexz binary or FUSE."""

import os
import pytest
from unittest.mock import patch
from hexz.mount import _MountPoint, mount


class TestMountFunction:
    """Test the mount() factory function."""

    def test_returns_mountpoint(self):
        mp = mount("test.hxz")
        assert isinstance(mp, _MountPoint)

    def test_path_is_absolute(self):
        mp = mount("test.hxz")
        assert os.path.isabs(mp.snapshot_path)

    def test_mount_with_mount_point(self, tmp_path):
        mp = mount("test.hxz", mount_point=str(tmp_path / "mnt"))
        assert mp.mount_point is not None
        assert os.path.isabs(mp.mount_point)

    def test_mount_with_none_mount_point(self):
        mp = mount("test.hxz")
        assert mp.mount_point is None

    def test_mount_with_custom_binary(self):
        mp = mount("test.hxz", binary="/usr/local/bin/hexz")
        assert mp.binary == "/usr/local/bin/hexz"


class TestMountPointInit:
    """Test _MountPoint initialization."""

    def test_default_binary(self):
        mp = _MountPoint("/tmp/test.hxz")
        assert mp.binary == "hexz"

    def test_custom_binary(self):
        mp = _MountPoint("/tmp/test.hxz", binary="/custom/hexz")
        assert mp.binary == "/custom/hexz"

    def test_mount_point_none(self):
        mp = _MountPoint("/tmp/test.hxz")
        assert mp.mount_point is None
        assert mp._temp_dir is None
        assert mp._process is None

    def test_mount_point_absolute(self, tmp_path):
        mp = _MountPoint("/tmp/test.hxz", mount_point=str(tmp_path))
        assert os.path.isabs(mp.mount_point)

    def test_path_property_before_mount(self):
        mp = _MountPoint("/tmp/test.hxz")
        assert mp.path is None

    def test_path_property_with_mount_point(self, tmp_path):
        mp = _MountPoint("/tmp/test.hxz", mount_point=str(tmp_path))
        assert mp.path == str(tmp_path)


class TestFindBinary:
    """Test _find_binary method."""

    @patch("shutil.which", return_value=None)
    @patch("os.path.exists", return_value=False)
    def test_binary_not_found(self, mock_exists, mock_which):
        mp = _MountPoint("/tmp/test.hxz", binary="nonexistent_xyz")
        with pytest.raises(FileNotFoundError, match="Could not find"):
            mp._find_binary()

    @patch("shutil.which", return_value="/usr/bin/hexz")
    def test_binary_found_in_path(self, mock_which):
        mp = _MountPoint("/tmp/test.hxz")
        result = mp._find_binary()
        assert result == "hexz"


class TestMountPointExit:
    """Test __exit__ cleanup."""

    def test_exit_with_no_process(self):
        mp = _MountPoint("/tmp/test.hxz")
        mp.mount_point = "/tmp/test_mount"
        mp._process = None
        # Should not raise
        mp.__exit__(None, None, None)

    def test_exit_with_no_mount_point(self):
        mp = _MountPoint("/tmp/test.hxz")
        mp.mount_point = None
        mp._process = None
        # Should not raise
        mp.__exit__(None, None, None)


class TestMountAll:
    """Test __all__ export."""

    def test_all_contains_mount(self):
        from hexz import mount as mount_mod

        assert hasattr(mount_mod, "__all__") is False or "mount" in getattr(
            mount_mod, "__all__", []
        )
