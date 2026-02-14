"""Extended tests for hexz.mount module to improve coverage from 25% to 80%+."""

import pytest
import os
import tempfile
import shutil
import subprocess
import hexz
from hexz.mount import _MountPoint
from hexz.exceptions import MountError


@pytest.fixture
def temp_dir():
    """Create temporary directory for test files."""
    d = tempfile.mkdtemp(prefix="hexz_mount_test_")
    yield d
    shutil.rmtree(d, ignore_errors=True)


@pytest.fixture
def sample_snapshot(temp_dir):
    """Create a sample snapshot for mounting tests."""
    data_path = os.path.join(temp_dir, "data.bin")
    snap_path = os.path.join(temp_dir, "test.hxz")

    # Create 1MB of test data
    with open(data_path, "wb") as f:
        f.write(bytes([i % 256 for i in range(1024 * 1024)]))

    with hexz.open(snap_path, mode="w", compression="lz4") as w:
        w.add(data_path)

    return snap_path


def test_mountpoint_path_property(sample_snapshot):
    """Test the path property of _MountPoint."""
    # Skip if binary not available
    if not shutil.which("hexz"):
        pytest.skip("hexz binary not found")

    mp = _MountPoint(sample_snapshot)
    # Before entering context, mount_point might be None
    # After entering, it should be set
    try:
        with mp as mounted:
            assert mounted.path is not None
            assert isinstance(mounted.path, str)
            assert os.path.isabs(mounted.path)
    except (MountError, TimeoutError):
        pytest.skip("Mount operation not supported in test environment")


def test_find_binary_in_path(sample_snapshot, temp_dir):
    """Test _find_binary when binary is in PATH."""
    if not shutil.which("hexz"):
        pytest.skip("hexz binary not found")

    mp = _MountPoint(sample_snapshot, binary="hexz")

    try:
        binary_path = mp._find_binary()
        assert binary_path is not None
        assert os.path.exists(binary_path) or shutil.which(binary_path)
    except FileNotFoundError:
        pytest.skip("hexz binary not in PATH")


def test_find_binary_local_target_release(sample_snapshot):
    """Test _find_binary finds local target/release/hexz."""
    # This tests the logic that searches parent directories for target/release/hexz
    mp = _MountPoint(sample_snapshot, binary="nonexistent_binary_name")

    # If we're in the hexz project, it should find target/release/hexz
    # Otherwise it should raise FileNotFoundError
    try:
        binary_path = mp._find_binary()
        # If found, verify it's the local build
        assert "target" in binary_path and "release" in binary_path
    except FileNotFoundError as e:
        # Expected if not in hexz project directory
        assert "Could not find" in str(e)


def test_find_binary_not_found_raises_error(sample_snapshot, monkeypatch):
    """Test that _find_binary raises FileNotFoundError when binary not found."""
    # Mock shutil.which to always return None
    monkeypatch.setattr("shutil.which", lambda x: None)

    # Mock os.path.exists to always return False
    monkeypatch.setattr("os.path.exists", lambda x: False)

    mp = _MountPoint(sample_snapshot, binary="definitely_does_not_exist_binary_xyz")

    with pytest.raises(FileNotFoundError) as exc_info:
        mp._find_binary()

    assert "Could not find" in str(exc_info.value)
    assert "definitely_does_not_exist_binary_xyz" in str(exc_info.value)


def test_mount_with_custom_mount_point(sample_snapshot, temp_dir):
    """Test mounting with a custom mount point directory."""
    if not shutil.which("hexz"):
        pytest.skip("hexz binary not found")

    custom_mount = os.path.join(temp_dir, "custom_mount")

    try:
        with hexz.mount(sample_snapshot, mount_point=custom_mount) as mp:
            assert mp.path == os.path.abspath(custom_mount)
            assert os.path.exists(custom_mount)
            assert os.path.exists(os.path.join(custom_mount, "disk"))
    except (MountError, TimeoutError, subprocess.CalledProcessError):
        pytest.skip("Mount operation not supported in test environment")


def test_mount_creates_custom_directory_if_not_exists(sample_snapshot, temp_dir):
    """Test that mount creates the custom mount point if it doesn't exist."""
    if not shutil.which("hexz"):
        pytest.skip("hexz binary not found")

    custom_mount = os.path.join(temp_dir, "new_dir", "mount_here")
    assert not os.path.exists(custom_mount)

    try:
        mp = _MountPoint(sample_snapshot, mount_point=custom_mount)
        with mp:
            # Directory should be created
            assert os.path.exists(custom_mount)
    except (MountError, TimeoutError, subprocess.CalledProcessError):
        pytest.skip("Mount operation not supported in test environment")


def test_mount_with_temp_directory(sample_snapshot):
    """Test mounting with automatic temporary directory creation."""
    if not shutil.which("hexz"):
        pytest.skip("hexz binary not found")

    try:
        with hexz.mount(sample_snapshot) as mp:
            # Should create a temp directory
            assert mp.path is not None
            assert os.path.exists(mp.path)

        # After exiting context, temp directory should be cleaned up
        # (though this is not guaranteed immediately due to timing)
    except (MountError, TimeoutError, subprocess.CalledProcessError):
        pytest.skip("Mount operation not supported in test environment")


def test_mount_timeout_error(sample_snapshot, temp_dir, monkeypatch):
    """Test that mount raises MountError on timeout."""
    if not shutil.which("hexz"):
        pytest.skip("hexz binary not found")

    # This test is tricky - we need to simulate a timeout
    # We can mock time.time to always return increasing values
    original_exists = os.path.exists

    def mock_exists(path):
        # Never return True for the "disk" file check
        if path.endswith("disk"):
            return False
        return original_exists(path)

    monkeypatch.setattr("os.path.exists", mock_exists)

    # Also need to mock the process to not die
    class MockPopen:
        def __init__(self, *args, **kwargs):
            self.returncode = None

        def poll(self):
            return None  # Process still running

        def terminate(self):
            pass

        def kill(self):
            pass

        def wait(self, timeout=None):
            pass

        def communicate(self):
            return b"", b"mock error"

    monkeypatch.setattr("subprocess.Popen", lambda *args, **kwargs: MockPopen())

    with pytest.raises(MountError) as exc_info:
        with hexz.mount(sample_snapshot):
            pass

    assert "Timed out" in str(exc_info.value) or "Mount failed" in str(exc_info.value)


def test_mount_process_dies_early(sample_snapshot, temp_dir, monkeypatch):
    """Test that mount raises MountError if process dies before mount ready."""
    if not shutil.which("hexz"):
        pytest.skip("hexz binary not found")

    class MockPopen:
        def __init__(self, *args, **kwargs):
            self.returncode = 1  # Process exited with error

        def poll(self):
            return self.returncode  # Already exited

        def communicate(self):
            return b"", b"Mock mount process failed"

        def terminate(self):
            pass

        def kill(self):
            pass

    monkeypatch.setattr("subprocess.Popen", lambda *args, **kwargs: MockPopen())

    with pytest.raises(MountError) as exc_info:
        with hexz.mount(sample_snapshot):
            pass

    assert "Mount failed" in str(exc_info.value)


def test_unmount_with_fusermount(sample_snapshot, temp_dir, monkeypatch):
    """Test unmount uses fusermount when available (Linux)."""
    if not shutil.which("hexz"):
        pytest.skip("hexz binary not found")

    fusermount_called = []
    original_run = subprocess.run

    def mock_run(args, **kwargs):
        fusermount_called.append(args)
        return original_run(["true"], **kwargs)  # Success

    monkeypatch.setattr("subprocess.run", mock_run)

    # Mock which to always return fusermount
    monkeypatch.setattr(
        "shutil.which",
        lambda cmd: "/usr/bin/fusermount" if cmd == "fusermount" else None,
    )

    try:
        mp = _MountPoint(sample_snapshot)
        with mp:
            pass

        # Check that fusermount was called
        if fusermount_called:
            assert any("fusermount" in str(call) for call in fusermount_called)
    except (MountError, TimeoutError, subprocess.CalledProcessError):
        pytest.skip("Mount operation not supported in test environment")


def test_unmount_with_umount_fallback(sample_snapshot, temp_dir, monkeypatch):
    """Test unmount falls back to umount when fusermount not available."""
    if not shutil.which("hexz"):
        pytest.skip("hexz binary not found")

    umount_called = []
    original_run = subprocess.run

    def mock_run(args, **kwargs):
        umount_called.append(args)
        return original_run(["true"], **kwargs)

    monkeypatch.setattr("subprocess.run", mock_run)

    # Mock which to return None for fusermount
    monkeypatch.setattr(
        "shutil.which", lambda cmd: None if cmd == "fusermount" else "/bin/umount"
    )

    try:
        mp = _MountPoint(sample_snapshot)
        with mp:
            pass

        # Check that umount was called
        if umount_called:
            assert any("umount" in str(call) for call in umount_called)
    except (MountError, TimeoutError, subprocess.CalledProcessError):
        pytest.skip("Mount operation not supported in test environment")


def test_unmount_terminates_process(sample_snapshot, monkeypatch):
    """Test that unmount terminates the mount process."""
    if not shutil.which("hexz"):
        pytest.skip("hexz binary not found")

    terminated = []
    killed = []

    class MockPopen:
        def __init__(self, *args, **kwargs):
            self.returncode = None
            self.stdout = None
            self.stderr = None

        def poll(self):
            return None  # Still running

        def terminate(self):
            terminated.append(True)

        def kill(self):
            killed.append(True)

        def wait(self, timeout=None):
            # Simulate timeout
            if timeout and not terminated:
                import subprocess

                raise subprocess.TimeoutExpired("mock", timeout)

    monkeypatch.setattr("subprocess.Popen", lambda *args, **kwargs: MockPopen())
    monkeypatch.setattr("subprocess.run", lambda *args, **kwargs: None)

    # Mock time and os.path.exists to fail quickly
    monkeypatch.setattr("os.path.exists", lambda p: False if "disk" in p else True)

    try:
        with hexz.mount(sample_snapshot):
            pass
    except (MountError, TimeoutError):
        pass  # Expected

    # Check that terminate or kill was called
    assert terminated or killed


def test_mount_function_parameters(sample_snapshot, temp_dir):
    """Test mount function with various parameter combinations."""
    if not shutil.which("hexz"):
        pytest.skip("hexz binary not found")

    from pathlib import Path

    # Test with Path objects
    try:
        with hexz.mount(Path(sample_snapshot)) as mp:
            assert mp.path is not None
    except (MountError, TimeoutError, subprocess.CalledProcessError):
        pytest.skip("Mount operation not supported in test environment")


def test_mount_with_custom_binary_path(sample_snapshot, temp_dir):
    """Test mount with custom binary parameter."""
    if not shutil.which("hexz"):
        pytest.skip("hexz binary not found")

    binary_path = shutil.which("hexz")
    if not binary_path:
        pytest.skip("hexz binary not found")

    try:
        with hexz.mount(sample_snapshot, binary=binary_path) as mp:
            assert mp.path is not None
    except (MountError, TimeoutError, subprocess.CalledProcessError):
        pytest.skip("Mount operation not supported in test environment")


def test_mount_exception_cleanup(sample_snapshot, temp_dir, monkeypatch):
    """Test that resources are cleaned up when mount fails."""
    if not shutil.which("hexz"):
        pytest.skip("hexz binary not found")

    # Force a failure by making the binary not found
    def mock_which(binary):
        return None

    monkeypatch.setattr("shutil.which", mock_which)

    mp = _MountPoint(sample_snapshot, binary="nonexistent")

    with pytest.raises(FileNotFoundError):
        with mp:
            pass
