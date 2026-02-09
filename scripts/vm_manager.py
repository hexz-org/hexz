import argparse
import logging
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Optional

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(levelname)s - %(message)s",
    datefmt="%H:%M:%S",
)
logger = logging.getLogger("strata_vm")


class StrataVMManager:
    def __init__(
        self, binary_path: Optional[str] = None, workspace_root: Optional[str] = None
    ):
        self.workspace_root = Path(workspace_root or os.getcwd())
        self.binary_path = self._find_binary(binary_path)
        self.data_dir = self.workspace_root / "data"
        self.output_dir = self.workspace_root / "vm"

        # Ensure directories exist
        self.data_dir.mkdir(exist_ok=True)
        self.output_dir.mkdir(exist_ok=True)

    def _find_binary(self, custom_path: Optional[str]) -> Path:
        """Locate the strata binary."""
        if custom_path:
            path = Path(custom_path)
            if path.exists() and os.access(path, os.X_OK):
                return path.absolute()
            raise FileNotFoundError(f"Custom binary not found at {custom_path}")

        # Check cargo target directory
        target_release = self.workspace_root / "target" / "release" / "strata"
        if target_release.exists():
            return target_release

        # Check PATH
        which_bin = shutil.which("strata")
        if which_bin:
            return Path(which_bin)

        raise FileNotFoundError(
            "Could not find 'strata' binary. Please run 'cargo build --release' or provide --binary-path."
        )

    def download_ubuntu(self, iso_name: str = "ubuntu-22.04.5-desktop-amd64.iso"):
        """Download Ubuntu ISO if not present."""
        iso_name = os.path.basename(iso_name)
        iso_path = self.data_dir / iso_name

        # Check if file exists and has content
        if iso_path.exists():
            if iso_path.stat().st_size > 0:
                logger.info(f"ISO already exists at {iso_path}")
                return iso_path
            else:
                logger.warning(f"Found empty ISO file at {iso_path}, deleting...")
                iso_path.unlink()

        url = f"https://releases.ubuntu.com/22.04/{iso_name}"
        logger.info(f"Downloading Ubuntu ISO from {url}...")

        try:
            subprocess.run(["wget", "-O", str(iso_path), url], check=True)
            logger.info("Download complete.")
        except (subprocess.CalledProcessError, FileNotFoundError):
            logger.error(
                "Failed to download ISO. Please install 'wget' or download manually."
            )
            sys.exit(1)

        return iso_path

    def install(
        self,
        iso_path: Path,
        disk_size: str = "10G",
        ram: str = "2G",
        output_name: str = "ubuntu.st",
        gui: bool = False,
        keep_iso: bool = False,
    ):
        """Run strata install."""
        output_name = os.path.basename(output_name)
        output_path = self.output_dir / output_name

        if output_path.exists():
            logger.warning(f"Output file {output_path} already exists.")
            if input("Overwrite? [y/N] ").lower() != "y":
                return output_path

        cmd = [
            str(self.binary_path),
            "vm",
            "install",
            "--iso",
            str(iso_path),
            "--disk-size",
            disk_size,
            "--ram",
            ram,
            "--output",
            str(output_path),
        ]

        if not gui:
            cmd.append("--no-graphics")

        logger.info(f"Running install command: {' '.join(cmd)}")
        try:
            # Install is interactive (console), so we just call it
            subprocess.run(cmd, check=True)
            logger.info(f"Installation successful: {output_path}")

            if not keep_iso and iso_path.exists():
                logger.info(f"Removing ISO file: {iso_path}")
                iso_path.unlink()

        except subprocess.CalledProcessError as e:
            logger.error(f"Installation failed with exit code {e.returncode}")
            sys.exit(e.returncode)

        return output_path

    def boot(
        self,
        snapshot_path: Path,
        ram: str = "2G",
        gui: bool = False,
        network: bool = True,
    ):
        """Run strata boot."""
        if not snapshot_path.exists():
            logger.error(f"Snapshot not found: {snapshot_path}")
            sys.exit(1)

        cmd = [
            str(self.binary_path),
            "vm",
            "boot",
            str(snapshot_path),
            "--ram",
            ram,
            "--backend",
            "qemu",
        ]

        if network:
            cmd.extend(["--network", "user"])
        else:
            cmd.extend(["--network", "none"])

        if not gui:
            cmd.append("--no-graphics")

        logger.info(f"Booting VM: {' '.join(cmd)}")
        try:
            subprocess.run(cmd, check=True)
        except subprocess.CalledProcessError as e:
            logger.error(f"Boot process exited with code {e.returncode}")


def main():
    parser = argparse.ArgumentParser(description="Strata VM Automation Script")
    parser.add_argument(
        "action", choices=["install", "boot", "all"], help="Action to perform"
    )
    parser.add_argument(
        "--iso", default="ubuntu-22.04.5-desktop-amd64.iso", help="ISO filename"
    )
    parser.add_argument("--disk-size", default="10G", help="Disk size (e.g., 10G)")
    parser.add_argument("--ram", default="2G", help="RAM size (e.g., 2G)")
    parser.add_argument("--snapshot", default="ubuntu.st", help="Snapshot filename")
    parser.add_argument("--binary", help="Path to strata binary")
    parser.add_argument(
        "--gui", action="store_true", help="Enable GUI (disable --no-graphics)"
    )
    parser.add_argument("--no-network", action="store_true", help="Disable networking")
    parser.add_argument(
        "--keep-iso", action="store_true", help="Keep ISO file after installation"
    )

    args = parser.parse_args()

    manager = StrataVMManager(binary_path=args.binary)

    if args.action in ["install", "all"]:
        iso_path = manager.download_ubuntu(args.iso)
        snap_path = manager.install(
            iso_path, args.disk_size, args.ram, args.snapshot, args.gui, args.keep_iso
        )

    if args.action in ["boot", "all"]:
        snap_path = manager.output_dir / os.path.basename(args.snapshot)
        manager.boot(snap_path, args.ram, args.gui, not args.no_network)


if __name__ == "__main__":
    main()
