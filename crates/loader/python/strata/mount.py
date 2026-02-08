import os
import shutil
import subprocess
import tempfile
import time
from typing import Optional


class Mount:
    """
    Context manager to mount a Strata snapshot.

    Usage:
        with strata.mount("my_snap.st") as mount_point:
            print(os.listdir(mount_point))
            # /tmp/tmp123/disk
            # /tmp/tmp123/memory
    """

    def __init__(
        self,
        snapshot_path: str,
        mount_point: Optional[str] = None,
        binary: str = "strata",
    ):
        self.snapshot_path = os.path.abspath(snapshot_path)
        # If mount_point is provided, we resolve it to an absolute path.
        # Callers must ensure this path is safe and trusted.
        self.mount_point = os.path.abspath(mount_point) if mount_point else None
        self.binary = binary
        self._temp_dir = None
        self._process = None

    def _find_binary(self):
        # Check if provided binary is in path
        if shutil.which(self.binary):
            return self.binary

        # Check local target/release (for dev)
        # Assuming we are running from project root or inside crates/py
        # Walk up until we find target or hit root
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
        self.binary_path = self._find_binary()

        if self.mount_point is None:
            self._temp_dir = tempfile.TemporaryDirectory()
            self.mount_point = self._temp_dir.name
        else:
            if not os.path.exists(self.mount_point):
                os.makedirs(self.mount_point)

        # Start strata mount in daemon mode (or background process)
        # We use the CLI: strata mount <SNAP> <MOUNTPOINT>
        cmd = [self.binary_path, "mount", self.snapshot_path, self.mount_point]

        # We don't use --daemon flag here because we want to own the process handle
        # to kill it easily on exit.
        self._process = subprocess.Popen(
            cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE
        )

        # Wait for mount to be ready
        # Simple heuristic: wait for 'disk' file to appear or timeout
        start = time.time()
        while time.time() - start < 5.0:
            if os.path.exists(os.path.join(self.mount_point, "disk")):
                return self.mount_point

            # Check if process died
            if self._process.poll() is not None:
                _, err = self._process.communicate()
                raise RuntimeError(f"Mount failed: {err.decode()}")

            time.sleep(0.1)

        self.__exit__(None, None, None)
        raise TimeoutError("Timed out waiting for mount")

    def __exit__(self, exc_type, exc_val, exc_tb):
        # Unmount
        if self.mount_point:
            # Try fusermount -u first (Linux)
            if shutil.which("fusermount"):
                subprocess.run(["fusermount", "-u", self.mount_point], check=False)
            else:
                subprocess.run(["umount", self.mount_point], check=False)

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
