"""
Script to create a dummy dataset for ML training example.
"""

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

    with strata.open(output_path, mode="w", packing="fast") as w:
        for i in range(num_items):
            size = np.random.randint(1024, 10240)
            data = np.random.bytes(size)
            w.add_bytes(data)

            index_entries.append((current_offset, size))
            current_offset += size

            if (i + 1) % 100 == 0:
                print(f"Processed {i + 1}/{num_items} items")

    # Write index file (offsets are logical, tracked as we add items)
    print(f"Writing index file to {index_path}...")
    with open(index_path, "wb") as f:
        for offset, size in index_entries:
            f.write(struct.pack("<QQ", offset, size))

    print("Dataset creation complete!")


if __name__ == "__main__":
    create_dummy_dataset()
