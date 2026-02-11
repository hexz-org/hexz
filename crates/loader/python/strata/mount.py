"""Filesystem mounting for Strata snapshots.

Provides utilities for mounting Strata snapshots as FUSE filesystems,
allowing direct file system access to snapshot contents.
"""

import os
import shutil
import subprocess
import tempfile
import time
from typing import Optional

from .typing import PathLike
from .exceptions import MountError


class _MountPoint:
    """Internal context manager to mount a Strata snapshot.

    Users should use the :func:`mount` function instead of instantiating this directly.

    Usage:
        with strata.mount("my_snap.st") as mp:
            print(os.listdir(mp.path))
            # /tmp/tmp123/disk
            # /tmp/tmp123/memory
    """

    def __init__(
        self,
        snapshot_path: str,
        mount_point: Optional[str] = None,
        binary: str = "strata",
    ):
        """Create a mount point.

        Args:
            snapshot_path: Path to .st file
            mount_point: Directory to mount at (creates temp if None)
            binary: Path to strata CLI binary
        """
        self.snapshot_path = os.path.abspath(snapshot_path)
        # If mount_point is provided, we resolve it to an absolute path.
        # Callers must ensure this path is safe and trusted.
        self.mount_point = os.path.abspath(mount_point) if mount_point else None
        self.binary = binary
        self._temp_dir = None
        self._process = None

    @property
    def path(self) -> str:
        """Get mount point path."""
        return self.mount_point

    def _find_binary(self):
        """Find the strata CLI binary."""
        # Check if provided binary is in path
        if shutil.which(self.binary):
            return self.binary

        # Check local target/release (for dev)
        curr = os.getcwd()
        while True:
            local_bin = os.path.join(curr, "target", "release", "strata")
            if os.path.exists(local_bin):
                return local_bin

            parent = os.path.dirname(curr)
            if parent == curr:
                break
            curr = parent

        raise FileNotFoundError(
            f"Could not find '{self.binary}' binary. Ensure it is installed or in PATH."
        )

    def __enter__(self):
        """Mount the snapshot."""
        self.binary_path = self._find_binary()

        if self.mount_point is None:
            self._temp_dir = tempfile.TemporaryDirectory()
            self.mount_point = self._temp_dir.name
        else:
            if not os.path.exists(self.mount_point):
                os.makedirs(self.mount_point)

        # Start strata vm mount in background (CLI uses strata vm mount)
        cmd = [self.binary_path, "vm", "mount", self.snapshot_path, self.mount_point]

        self._process = subprocess.Popen(
            cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE
        )

        # Wait for mount to be ready
        start = time.time()
        while time.time() - start < 5.0:
            if os.path.exists(os.path.join(self.mount_point, "disk")):
                return self

            # Check if process died
            if self._process.poll() is not None:
                _, err = self._process.communicate()
                if self._temp_dir:
                    self._temp_dir.cleanup()
                raise MountError(f"Mount failed: {err.decode()}")

            time.sleep(0.1)

        self.__exit__(None, None, None)
        raise MountError("Timed out waiting for mount")

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Unmount and cleanup."""
        # Unmount
        if self.mount_point:
            # Try fusermount -u first (Linux)
            if shutil.which("fusermount"):
                subprocess.run(
                    ["fusermount", "-u", self.mount_point],
                    check=False,
                    capture_output=True,
                )
            else:
                subprocess.run(
                    ["umount", self.mount_point], check=False, capture_output=True
                )

        # Terminate process if still running
        if self._process and self._process.poll() is None:
            self._process.terminate()
            try:
                self._process.wait(timeout=1)
            except subprocess.TimeoutExpired:
                self._process.kill()

        # Cleanup temp dir
        if self._temp_dir:
            self._temp_dir.cleanup()


def mount(
    snapshot: PathLike,
    *,
    mount_point: Optional[PathLike] = None,
    binary: str = "strata",
) -> _MountPoint:
    """Mount a Strata snapshot as a filesystem.

    The snapshot is automatically unmounted when exiting the context manager.

    Args:
        snapshot: Path to .st file
        mount_point: Directory to mount at (creates temp if None)
        binary: Path to strata CLI binary

    Returns:
        Context manager that provides mount point access

    Example:
        >>> with strata.mount("snapshot.st") as mp:
        ...     files = os.listdir(mp.path)
        ...     print(files)
        ... # Automatically unmounted here
    """
    return _MountPoint(
        str(snapshot),
        mount_point=str(mount_point) if mount_point else None,
        binary=binary,
    )


# unmount() has been removed in v0.1.0-beta
# Use the mount() context manager which automatically unmounts on exit


__all__ = ["mount"]  # Only export mount(), MountPoint is internal
