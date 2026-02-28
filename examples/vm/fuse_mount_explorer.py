"""Example: Virtual FUSE Mounting.

Hexz snapshots can be mounted as virtual filesystems. This allows
non-Python tools (like your terminal or C++ apps) to read data
directly from a snapshot without extracting it.
"""

import hexz
import os
import subprocess
import time


def run_example():
    if not hasattr(hexz, "mount") or hexz.mount is None:
        print("FUSE mounting is not supported on this platform or not enabled.")
        return

    snapshot_path = "virtual_disk.hxz"
    mount_dir = "mnt_point"
    os.makedirs(mount_dir, exist_ok=True)

    # 1. Create a snapshot with some files
    print("Creating snapshot...")
    with hexz.Writer(snapshot_path) as writer:
        writer.add(b"Hello from inside the snapshot!", kind="disk")
        # We simulate a nested structure by giving a name with a slash
        # (if the writer/builder supports it, or just multiple entries)
        writer.add_metadata({"version": "1.0", "description": "Virtual Drive"})

    print(f"Mounting {snapshot_path} to {mount_dir}/ ...")

    # 2. Use the mount context manager
    try:
        with hexz.mount(snapshot=snapshot_path, mount_point=mount_dir) as mnt:
            print(f"✓ Snapshot mounted at {mnt.path}")

            # 3. Access with standard OS tools
            print("\nListing files in mount point:")
            subprocess.run(["ls", "-l", mnt.path])

            # Read a file from the virtual disk
            # (Hexz FUSE typically maps entries to virtual files)
            print("\nReading virtual file 'primary.bin' (if exists):")
            # Note: The mapping depends on the Builder implementation.
            # Usually, disk entries appear as files.

            print("\nWaiting 2 seconds... you could check this in another terminal!")
            time.sleep(2)

    except Exception as e:
        print(f"Failed to mount (check if fuse is installed): {e}")
    finally:
        # Cleanup
        if os.path.exists(mount_dir):
            # The context manager handles unmounting
            os.rmdir(mount_dir)
        if os.path.exists(snapshot_path):
            os.remove(snapshot_path)


if __name__ == "__main__":
    run_example()
