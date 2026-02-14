#!/usr/bin/env python3
"""
Quick start: create a Hexz snapshot and read it back in under a minute.

Run from repo root (with hexz installed via maturin develop from crates/loader):
    python examples/quickstart.py

Or from examples/:
    python quickstart.py
"""

import os
import tempfile

import hexz


def main():
    print("Hexz quick start — 5 minutes to first result\n")
    version = hexz.version()

    print(f"You are using version {version}")

    # Use a temp dir so we don't leave files behind
    with tempfile.TemporaryDirectory(prefix="hexz_quickstart_") as tmp:
        data_path = os.path.join(tmp, "hello.bin")
        snap_path = os.path.join(tmp, "hello.hxz")

        # 1. Create a file to pack (large enough that compression beats format overhead)
        chunk = b"Hello, Hexz! " * 64  # 960 bytes, highly compressible
        with open(data_path, "wb") as f:
            f.write(chunk * 53)  # ~51 KB
        original_size = os.path.getsize(data_path)

        # 2. Build a snapshot (Python API)
        print("Building snapshot...")
        with hexz.open(snap_path, mode="w", compression="lz4") as w:
            w.add(data_path)
        st_size = os.path.getsize(snap_path)

        # Show why Hexz is useful: original vs compressed size
        pct = (st_size / original_size * 100) if original_size else 0
        print(f"  Original:  {original_size:,} bytes")
        print(f"  Snapshot:  {st_size:,} bytes  ({pct:.0f}% of original)")
        print()

        # 3. Open and read — your first result
        print("Reading back...")
        # Note: prefetch=True by default for sequential reads
        # Set prefetch=False to disable background prefetching
        with hexz.open(snap_path) as reader:
            chunk = reader.read(64)  # from start (cursor at 0)
            # Random access: reader.read(size, offset=...) does not move the cursor
            same = reader.read(64, offset=0)
        print(f"  First 64 bytes: {chunk!r}")
        assert chunk == same

        # 4. Optional: inspect metadata
        meta = hexz.inspect(snap_path)
        print(
            f"  Metadata: {meta.num_blocks} block(s), logical size {meta.disk_size} bytes"
        )

    print("\nDone! You built a snapshot and read from it with hexz.open().")
    print("Next: try hexz.build() for folders, or hexz.Dataset() for ML training.")


if __name__ == "__main__":
    main()
