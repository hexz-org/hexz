"""
Demonstrate advanced build options with Strata.
"""

import strata
from pathlib import Path


def advanced_build_demo(source_dir: str):
    print(f"Building advanced snapshot from {source_dir}...")

    output_file = "advanced.st"

    # Custom profile:
    # Use 'tight' packing but override block size for sequential read optimization
    # and use zstd level 19 for maximum compression.

    print("\nConfiguration:")
    print("- Base profile: archival")
    print("- Block size: 512KB (Override)")
    print("- Compression: zstd (from profile)")
    print("- Dedup: Enabled (from profile)")

    # Note: 'compression_level' is not directly exposed in build() yet
    # but could be passed via **overrides if Writer supports it.
    # Writer __init__ takes 'compression_level' via config dict if passed.
    # Let's check Writer implementation.
    # Writer takes: compression, mode/packing, block_size, dedup, encrypt, password.
    # It maps mode to compression level internally.
    # So we can't easily override compression level directly unless we add it to Writer.

    # However, we can override block_size and packing mode.

    try:
        meta = strata.build(
            source_dir,
            output_file,
            profile="archival",
            block_size=512 * 1024,  # Override block size
            packing="tight",  # Explicitly set packing mode (redundant with archival but valid)
        )

        print("\nBuild complete!")
        print(f"File: {output_file}")
        print(f"Size: {meta.size_compressed / 1024 / 1024:.2f} MB")
        print(f"Original size: {meta.disk_size / 1024 / 1024:.2f} MB")
        print(f"Ratio: {meta.compression_ratio:.2f}")
        print(f"Block size: {meta.block_size}")

    except Exception as e:
        print(f"Build failed: {e}")

    # Inspect detailed metadata
    print("\nDetailed Inspection:")
    info = strata.inspect(output_file)
    print(f"Version: {info.version}")
    print(f"Compression: {info.compression}")
    print(f"Is Compatible: {info.is_compatible}")

    # Verify (mock)
    # verify(output_file)

    # Cleanup
    import os

    os.remove(output_file)


if __name__ == "__main__":
    import shutil

    # Use dedup_data/base as source
    project_root = Path(__file__).parent.parent.parent
    source_dir = project_root / "dedup_data/base"

    created_dummy = False
    if not source_dir.exists():
        print(f"Source directory {source_dir} not found. generating dummy data...")
        source_dir = Path("dummy_data")
        source_dir.mkdir(exist_ok=True)
        created_dummy = True

        # Generate some compressible data
        for i in range(5):
            with open(source_dir / f"text_{i}.txt", "w") as f:
                f.write(
                    ("This is some repeated text to test compression. " * 100 + "\n")
                    * 1000
                )

        # Generate some binary data with patterns
        for i in range(5):
            with open(source_dir / f"binary_{i}.bin", "wb") as f:
                f.write(b"\x00" * 10000 + b"\xff" * 10000)

    advanced_build_demo(str(source_dir))

    if created_dummy:
        shutil.rmtree(source_dir)
