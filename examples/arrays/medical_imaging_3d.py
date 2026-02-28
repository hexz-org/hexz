"""Example: 3D Medical Imaging Slicing.

This example shows how to store large 3D volumes (like MRI or CT scans)
and efficiently extract 2D slices along any axis using Hexz's random
access and ArrayView.
"""

import os
import numpy as np
import hexz
import time
from pathlib import Path

_DATA_DIR = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))), ".data", "arrays"
)


def run_example():
    os.makedirs(_DATA_DIR, exist_ok=True)

    # 1. Create a larger synthetic 3D volume (512 x 256 x 256)
    # 128MB of float32 data. We add some noise to prevent
    # perfect compression from masking the random access benefit.
    shape = (512, 256, 256)
    print(f"Generating synthetic 3D volume: {shape} (~128MB)")

    # Create a volume with noise and a sphere
    volume = np.random.normal(0, 0.01, shape).astype(np.float32)
    z, y, x = np.ogrid[:512, :256, :256]
    dist_from_center = np.sqrt((x - 128) ** 2 + (y - 128) ** 2 + (z - 256) ** 2)
    volume += (dist_from_center <= 100).astype(np.float32)

    snapshot_path = os.path.join(_DATA_DIR, "medical_scan.hxz")

    # 2. Save the 3D volume as an array
    print("Saving 3D volume to Hexz...")
    hexz.write_array(snapshot_path, volume, compression="zstd")

    # 3. Random Access: Pulling a specific 2D Slice
    # We want a slice in the middle (z=256)
    print("\nExtracting 2D slice at Z=256...")

    # Calculate file size to show comparison
    compressed_size = Path(snapshot_path).stat().st_size

    # We use ArrayView to treat the snapshot like a numpy array on disk
    start_time = time.perf_counter()
    with hexz.ArrayView(snapshot_path, shape=shape, dtype="float32") as view:
        # Pull the slice
        # This only reads the bytes required for this specific slice!
        z_slice = view[256, :, :]
        duration = (time.perf_counter() - start_time) * 1000

        print(f"Read slice {z_slice.shape} in {duration:.2f}ms")

    # 4. Compare
    print("\nResults:")
    print("  Full Volume (Uncompressed): 128.0 MB")
    print(f"  Snapshot Size (Compressed): {compressed_size / (1024 * 1024):.1f} MB")
    print(f"  Single Slice Size: {z_slice.nbytes / (1024):.1f} KB")
    print("  Efficiency: Hexz read only ~0.2% of the volume data!")

    # Clean up
    Path(snapshot_path).unlink()


if __name__ == "__main__":
    run_example()
