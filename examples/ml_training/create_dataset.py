"""
Script to create a dummy dataset for ML training example.
"""

import os
import struct
import numpy as np
import strata
from pathlib import Path


def create_dummy_dataset(output_path: str = "dataset.st", num_items: int = 1000):
    print(f"Creating dummy dataset with {num_items} items...")

    # We will simulate variable length items (e.g. text or compressed images)
    # So we need to create an index file
    index_path = Path(output_path).with_suffix(".idx")

    current_offset = 0
    index_entries = []

    with strata.Writer(output_path, packing="fast") as w:
        for i in range(num_items):
            # Simulate random image size between 1KB and 10KB
            size = np.random.randint(1024, 10240)
            data = np.random.bytes(size)

            # Write to strata file
            # Note: In a real scenario, we would add the file directly.
            # But since we are generating random bytes, we use add_bytes (via temp file workaround for now)
            # Or add_array if we treat it as numpy array.

            # For this example, let's write temporary files and add them
            # This is inefficient but works until add_bytes is native
            temp_name = f"temp_{i}.bin"
            with open(temp_name, "wb") as f:
                f.write(data)

            w.add_file(temp_name)
            os.remove(temp_name)

            # Record index entry
            # Note: We need to know the offset in the final file.
            # Currently Strata Writer doesn't return the offset of the added item easily
            # without inspecting the metadata afterwards or calculating it.
            # However, Strata files are compressed, so we can't easily guess the offset
            # unless we are writing uncompressed or we get it from the writer.

            # WAIT: The current Writer implementation doesn't return the offset.
            # And for compressed streams, offsets are logical vs physical.
            # The Dataset class expects logical offsets if it's reading a stream,
            # or physical if it's reading blocks.

            # Reader.read_at(offset, length) takes a LOGICAL offset in the uncompressed stream.
            # So we just need to track the uncompressed size we've written.

            index_entries.append((current_offset, size))
            current_offset += size

            if (i + 1) % 100 == 0:
                print(f"Processed {i + 1}/{num_items} items")

    # Write index file
    print(f"Writing index file to {index_path}...")
    with open(index_path, "wb") as f:
        for offset, size in index_entries:
            f.write(struct.pack("<QQ", offset, size))

    print("Dataset creation complete!")


if __name__ == "__main__":
    create_dummy_dataset()
