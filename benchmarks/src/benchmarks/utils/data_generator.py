#!/usr/bin/env python3
"""
Generate realistic test data for benchmarking.

Creates deterministic, moderately compressible data that mimics real ML datasets.
"""

import argparse
import json
from pathlib import Path
from typing import Dict

import numpy as np
from tqdm import tqdm


def generate_compressible_data(
    size: int, seed: int = 42, entropy: float = 0.6
) -> bytes:
    """
    Generate data with controlled entropy (compressibility).

    Args:
        size: Number of bytes to generate
        seed: Random seed for reproducibility
        entropy: Entropy level (0.0 = highly compressible, 1.0 = incompressible)

    Returns:
        Bytes with specified entropy level
    """
    rng = np.random.RandomState(seed)

    if entropy < 0.3:
        # Highly compressible: repeating patterns
        pattern_size = max(1, int(size * (1 - entropy)))
        pattern = rng.bytes(pattern_size)
        num_repeats = (size // pattern_size) + 1
        data = (pattern * num_repeats)[:size]
    elif entropy < 0.7:
        # Moderately compressible: mix of patterns and random
        # Simulates JPEG-like data
        num_blocks = size // 64
        blocks = []
        for i in range(num_blocks):
            if i % 3 == 0:
                # Repeating block
                block = b"\x00" * 64 if i % 6 == 0 else rng.bytes(16) * 4
            else:
                # Random block
                block = rng.bytes(64)
            blocks.append(block)

        # Fill remainder
        remainder = size - (num_blocks * 64)
        if remainder > 0:
            blocks.append(rng.bytes(remainder))

        data = b"".join(blocks)
    else:
        # Low compressibility: mostly random
        data = rng.bytes(size)

    return data


def generate_image_like_sample(
    width: int, height: int, channels: int, seed: int
) -> bytes:
    """
    Generate image-like data with spatial correlation.

    Simulates compressed image data (like JPEG) with:
    - Smooth gradients (low frequency)
    - Some high-frequency details
    - Moderate compressibility (~60%)
    """
    rng = np.random.RandomState(seed)

    # Create smooth base with gradients
    x = np.linspace(0, 1, width)
    y = np.linspace(0, 1, height)
    xx, yy = np.meshgrid(x, y)

    image = np.zeros((height, width, channels), dtype=np.uint8)

    for c in range(channels):
        # Smooth gradient
        gradient = (
            np.sin(xx * 2 * np.pi + c) * 0.3 + np.cos(yy * 2 * np.pi + c) * 0.3 + 0.5
        )

        # Add some noise
        noise = rng.randn(height, width) * 0.1

        # Combine and normalize to uint8
        channel_data = gradient + noise
        channel_data = np.clip(channel_data * 255, 0, 255).astype(np.uint8)
        image[:, :, c] = channel_data

    return image.tobytes()


class TestDataGenerator:
    """Generate realistic test datasets for benchmarking."""

    def __init__(self, output_dir: Path):
        self.output_dir = output_dir
        self.output_dir.mkdir(parents=True, exist_ok=True)
        self.raw_dir = output_dir / "raw"
        self.raw_dir.mkdir(exist_ok=True)

    def generate_dataset(
        self,
        name: str,
        num_samples: int,
        sample_size: int,
        variable_size: bool = False,
        image_like: bool = False,
    ) -> Dict:
        """
        Generate a complete dataset.

        Args:
            name: Dataset name
            num_samples: Number of samples
            sample_size: Base size per sample in bytes
            variable_size: If True, vary sample sizes ±50%
            image_like: If True, generate image-like patterns

        Returns:
            Metadata dict with dataset info
        """
        print(f"\n📦 Generating dataset: {name}")
        print(f"   Samples: {num_samples:,}")
        print(f"   Size per sample: {sample_size:,} bytes")

        dataset_dir = self.raw_dir / name
        dataset_dir.mkdir(exist_ok=True)

        manifest = []
        total_bytes = 0

        for i in tqdm(range(num_samples), desc=f"  Creating {name}"):
            # Determine sample size
            if variable_size:
                # Vary size by ±50%
                size = int(sample_size * (0.5 + np.random.rand()))
            else:
                size = sample_size

            # Generate sample data
            if image_like and size >= 256:
                # Generate as small image
                channels = 3
                pixels = size // channels
                side = int(np.sqrt(pixels))
                if side * side * channels <= size:
                    data = generate_image_like_sample(side, side, channels, seed=i)
                    # Pad if needed
                    if len(data) < size:
                        data += b"\x00" * (size - len(data))
                    else:
                        data = data[:size]
                else:
                    data = generate_compressible_data(size, seed=i, entropy=0.6)
            else:
                # Generate compressible data
                data = generate_compressible_data(size, seed=i, entropy=0.6)

            # Write sample
            sample_path = dataset_dir / f"sample_{i:06d}.bin"
            with open(sample_path, "wb") as f:
                f.write(data)

            # Create label (synthetic)
            label = i % 1000  # 1000 classes

            manifest.append(
                {
                    "id": i,
                    "path": str(sample_path.relative_to(self.output_dir)),
                    "size": len(data),
                    "label": label,
                }
            )
            total_bytes += len(data)

        # Write manifest
        manifest_path = dataset_dir / "manifest.json"
        with open(manifest_path, "w") as f:
            json.dump(manifest, f, indent=2)

        metadata = {
            "name": name,
            "num_samples": num_samples,
            "total_size": total_bytes,
            "avg_size": total_bytes // num_samples,
            "variable_size": variable_size,
            "image_like": image_like,
            "manifest": str(manifest_path.relative_to(self.output_dir)),
        }

        print(f"   ✓ Generated {total_bytes / 1024 / 1024:.1f} MB")

        return metadata

    def generate_all(self) -> Dict:
        """Generate all test datasets."""
        datasets = {}

        # 1. Small fixed-size samples (like CIFAR-10)
        datasets["cifar_like"] = self.generate_dataset(
            name="cifar_like",
            num_samples=50000,
            sample_size=4096,  # 4KB ~= 32x32x3
            variable_size=False,
            image_like=True,
        )

        # 2. Medium images (like ImageNet)
        datasets["imagenet_like"] = self.generate_dataset(
            name="imagenet_like",
            num_samples=10000,
            sample_size=51200,  # 50KB (typical compressed JPEG)
            variable_size=False,
            image_like=True,
        )

        # 3. Variable-size dataset
        datasets["variable_size"] = self.generate_dataset(
            name="variable_size",
            num_samples=20000,
            sample_size=8192,  # 8KB average
            variable_size=True,
            image_like=False,
        )

        # 4. Small dataset for quick tests
        datasets["tiny"] = self.generate_dataset(
            name="tiny",
            num_samples=1000,
            sample_size=4096,
            variable_size=False,
            image_like=True,
        )

        # Save overall metadata
        metadata_path = self.output_dir / "datasets.json"
        with open(metadata_path, "w") as f:
            json.dump(datasets, f, indent=2)

        # Print summary
        total_size = sum(d["total_size"] for d in datasets.values())
        total_samples = sum(d["num_samples"] for d in datasets.values())

        print("\n" + "=" * 60)
        print("📊 Summary")
        print("=" * 60)
        print(f"Total datasets: {len(datasets)}")
        print(f"Total samples: {total_samples:,}")
        print(f"Total size: {total_size / 1024 / 1024:.1f} MB")
        print(f"Metadata: {metadata_path}")
        print("=" * 60)

        return datasets


def main():
    parser = argparse.ArgumentParser(description="Generate test data for benchmarks")
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path(__file__).parent / "data",
        help="Output directory for test data",
    )
    parser.add_argument(
        "--quick",
        action="store_true",
        help="Generate only tiny dataset for quick testing",
    )

    args = parser.parse_args()

    print("🔧 Test Data Generator")
    print(f"Output directory: {args.output_dir}")

    generator = TestDataGenerator(args.output_dir)

    if args.quick:
        # Just generate tiny dataset
        datasets = {
            "tiny": generator.generate_dataset(
                name="tiny",
                num_samples=100,
                sample_size=4096,
                variable_size=False,
                image_like=True,
            )
        }
        metadata_path = args.output_dir / "datasets.json"
        with open(metadata_path, "w") as f:
            json.dump(datasets, f, indent=2)
    else:
        datasets = generator.generate_all()

    print("\n✅ Test data generation complete!")
    print("\nNext steps:")
    print("  1. Run benchmarks: python run_all_benchmarks.py")
    print("  2. View results: cat results/*.json")


if __name__ == "__main__":
    main()
