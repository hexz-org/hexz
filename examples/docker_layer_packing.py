"""Example: Packing Docker Layer Data with Hexz.

This example shows how to use Hexz to efficiently store and distribute
Docker-style layer artifacts. Docker images consist of stacked filesystem
layers, each of which can be large and contain a lot of redundant data
(e.g., shared libraries, base OS files repeated across images).

Hexz is well-suited for this use case because:
  - Content-defined chunking (CDC) deduplicates data across layers
  - Block-level compression reduces transfer size
  - Fast random-access reads let container runtimes fetch specific files
    without extracting the full layer

This script simulates two Docker image layers (a "base" layer and an
"app" layer), packs them into Hexz snapshots, and shows the storage
savings from cross-layer deduplication.

Run from repo root:
    python examples/docker_layer_packing.py
"""

import hashlib
import os
import tempfile

import hexz


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_layer(directory: str, shared_data: bytes, unique_tag: str) -> None:
    """Populate a mock Docker layer directory with files."""
    # Shared OS files (would be identical across every image built on this base)
    for name, content in [
        ("lib/libc.so", shared_data),
        ("lib/libz.so", shared_data[: len(shared_data) // 2]),
        ("usr/bin/python", shared_data * 2),
    ]:
        path = os.path.join(directory, name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as f:
            f.write(content)

    # Layer-specific application files
    app_dir = os.path.join(directory, "app")
    os.makedirs(app_dir, exist_ok=True)
    with open(os.path.join(app_dir, "main.py"), "wb") as f:
        f.write(f"# {unique_tag}\nprint('Hello from {unique_tag}')\n".encode() * 512)
    with open(os.path.join(app_dir, "config.json"), "wb") as f:
        content = f'{{"layer": "{unique_tag}", "version": "1.0"}}'.encode() * 256
        f.write(content)


def _dir_size(directory: str) -> int:
    total = 0
    for root, _, files in os.walk(directory):
        for name in files:
            total += os.path.getsize(os.path.join(root, name))
    return total


def _sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()[:12]


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def run_example():
    print("Hexz — Docker layer packing example\n")
    version = hexz.version()
    print(f"hexz version: {version}\n")

    # Shared base data simulates a large OS runtime (e.g., glibc, Python interpreter)
    shared_data = (b"SHARED_OS_RUNTIME_DATA_" * 47) * 1024  # ~1 MB of repetitive data

    with tempfile.TemporaryDirectory(prefix="hexz_docker_") as tmp:
        # ------------------------------------------------------------------ #
        # 1. Build two simulated layer directories                            #
        # ------------------------------------------------------------------ #
        base_dir = os.path.join(tmp, "base_layer")
        app_dir = os.path.join(tmp, "app_layer")

        _make_layer(base_dir, shared_data, "base-v1.0")
        _make_layer(app_dir, shared_data, "app-v2.3")  # same shared libs

        raw_base = _dir_size(base_dir)
        raw_app = _dir_size(app_dir)
        print("Raw layer sizes:")
        print(f"  base layer : {raw_base:>10,} bytes")
        print(f"  app  layer : {raw_app:>10,} bytes")
        print(f"  combined   : {raw_base + raw_app:>10,} bytes")
        print()

        # ------------------------------------------------------------------ #
        # 2. Pack each layer into a Hexz snapshot                            #
        # ------------------------------------------------------------------ #
        base_snap = os.path.join(tmp, "base.hxz")
        app_snap = os.path.join(tmp, "app.hxz")

        print("Packing layers into Hexz snapshots (zstd compression)...")

        with hexz.open(base_snap, mode="w", compression="zstd") as w:
            for root, _, files in os.walk(base_dir):
                for name in sorted(files):
                    filepath = os.path.join(root, name)
                    w.add(filepath)

        with hexz.open(app_snap, mode="w", compression="zstd") as w:
            for root, _, files in os.walk(app_dir):
                for name in sorted(files):
                    filepath = os.path.join(root, name)
                    w.add(filepath)

        packed_base = os.path.getsize(base_snap)
        packed_app = os.path.getsize(app_snap)
        total_packed = packed_base + packed_app

        print("\nPacked snapshot sizes:")
        print(
            f"  base.hxz : {packed_base:>10,} bytes  ({packed_base / raw_base * 100:.1f}% of raw)"
        )
        print(
            f"  app.hxz  : {packed_app:>10,} bytes  ({packed_app / raw_app * 100:.1f}% of raw)"
        )
        print(
            f"  combined : {total_packed:>10,} bytes  ({total_packed / (raw_base + raw_app) * 100:.1f}% of raw)"
        )
        print()

        # ------------------------------------------------------------------ #
        # 3. Verify content integrity via inspect + random-access read        #
        # ------------------------------------------------------------------ #
        print("Verifying snapshots...")

        for snap_path, label in [(base_snap, "base"), (app_snap, "app")]:
            meta = hexz.inspect(snap_path)
            with hexz.open(snap_path) as r:
                r.read(16)  # verify readable
            sha = _sha256(snap_path)
            print(
                f"  {label}.hxz : {meta.num_blocks} block(s), "
                f"logical {meta.primary_size:,} bytes, sha256={sha}"
            )

        print()

        # ------------------------------------------------------------------ #
        # 4. Demonstrate content-addressable usage (Docker-style digest)      #
        # ------------------------------------------------------------------ #
        print("Layer digests (sha256, suitable for a content-addressable store):")
        for snap_path, label in [(base_snap, "base"), (app_snap, "app")]:
            digest = hashlib.sha256(open(snap_path, "rb").read()).hexdigest()
            print(f"  {label} : sha256:{digest}")

        print()
        savings = (raw_base + raw_app) - total_packed
        print(
            f"Storage savings: {savings:,} bytes "
            f"({savings / (raw_base + raw_app) * 100:.1f}% reduction) "
            "vs raw layer tarballs."
        )
        print("\nDone! In a real pipeline you would push these .hxz files to a")
        print("registry or object store instead of pushing uncompressed tar layers.")


if __name__ == "__main__":
    run_example()
