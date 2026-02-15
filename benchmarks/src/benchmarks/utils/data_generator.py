#!/usr/bin/env python3
"""
Download real datasets for benchmarking.

Uses actual CIFAR-10, STL-10, and CIFAR-100 images so compression
characteristics reflect real-world ML workloads.
"""

import argparse
import io
import json
from pathlib import Path
from typing import Dict

from PIL import Image
from tqdm import tqdm


class RealDataGenerator:
    """Download and prepare real datasets for benchmarking."""

    def __init__(self, output_dir: Path):
        self.output_dir = output_dir
        self.output_dir.mkdir(parents=True, exist_ok=True)
        self.raw_dir = output_dir / "raw"
        self.raw_dir.mkdir(exist_ok=True)
        # Cache dir for torchvision downloads
        self._cache_dir = output_dir / ".cache"
        self._cache_dir.mkdir(exist_ok=True)

    def _save_image_as_bytes(
        self, img: Image.Image, path: Path, fmt: str = "PNG"
    ) -> int:
        """Save a PIL image to disk and return the file size in bytes."""
        buf = io.BytesIO()
        img.save(buf, format=fmt)
        data = buf.getvalue()
        with open(path, "wb") as f:
            f.write(data)
        return len(data)

    def prepare_cifar10(self, name: str = "cifar10", max_samples: int = 50000) -> Dict:
        """
        Download CIFAR-10 and save each image as a PNG file.

        CIFAR-10: 50,000 training images, 32x32x3, 10 classes.
        PNG files are ~1-3KB each — real compression characteristics.
        """
        from torchvision.datasets import CIFAR10

        print(f"\n📦 Downloading CIFAR-10 ({max_samples} samples)...")
        dataset = CIFAR10(root=str(self._cache_dir), train=True, download=True)

        dataset_dir = self.raw_dir / name
        dataset_dir.mkdir(exist_ok=True)

        manifest = []
        total_bytes = 0
        num_samples = min(max_samples, len(dataset))

        for i in tqdm(range(num_samples), desc=f"  Saving {name}"):
            img, label = dataset[i]
            sample_path = dataset_dir / f"sample_{i:06d}.png"
            size = self._save_image_as_bytes(img, sample_path)

            manifest.append(
                {
                    "id": i,
                    "path": str(sample_path.relative_to(self.output_dir)),
                    "size": size,
                    "label": int(label),
                }
            )
            total_bytes += size

        manifest_path = dataset_dir / "manifest.json"
        with open(manifest_path, "w") as f:
            json.dump(manifest, f, indent=2)

        avg_size = total_bytes // num_samples if num_samples > 0 else 0
        metadata = {
            "name": name,
            "num_samples": num_samples,
            "total_size": total_bytes,
            "avg_size": avg_size,
            "variable_size": True,  # PNG sizes vary per image content
            "image_like": True,
            "manifest": str(manifest_path.relative_to(self.output_dir)),
            "source": "CIFAR-10 (torchvision)",
        }

        print(
            f"   ✓ {num_samples} images, {total_bytes / 1024 / 1024:.1f} MB total, ~{avg_size} bytes avg"
        )
        return metadata

    def prepare_stl10(self, name: str = "stl10", max_samples: int = 10000) -> Dict:
        """
        Download STL-10 and save each image as a JPEG file.

        STL-10: 5,000 labeled train + 100,000 unlabeled images, 96x96x3.
        JPEG files are ~5-15KB each — closer to ImageNet-scale sample sizes.
        Uses the unlabeled split for more data.
        """
        from torchvision.datasets import STL10

        print(f"\n📦 Downloading STL-10 ({max_samples} samples)...")
        # Use unlabeled split for larger dataset
        dataset = STL10(root=str(self._cache_dir), split="unlabeled", download=True)

        dataset_dir = self.raw_dir / name
        dataset_dir.mkdir(exist_ok=True)

        manifest = []
        total_bytes = 0
        num_samples = min(max_samples, len(dataset))

        for i in tqdm(range(num_samples), desc=f"  Saving {name}"):
            img, label = dataset[i]
            sample_path = dataset_dir / f"sample_{i:06d}.jpg"
            size = self._save_image_as_bytes(img, sample_path, fmt="JPEG")

            manifest.append(
                {
                    "id": i,
                    "path": str(sample_path.relative_to(self.output_dir)),
                    "size": size,
                    "label": int(label),
                }
            )
            total_bytes += size

        manifest_path = dataset_dir / "manifest.json"
        with open(manifest_path, "w") as f:
            json.dump(manifest, f, indent=2)

        avg_size = total_bytes // num_samples if num_samples > 0 else 0
        metadata = {
            "name": name,
            "num_samples": num_samples,
            "total_size": total_bytes,
            "avg_size": avg_size,
            "variable_size": True,
            "image_like": True,
            "manifest": str(manifest_path.relative_to(self.output_dir)),
            "source": "STL-10 unlabeled (torchvision)",
        }

        print(
            f"   ✓ {num_samples} images, {total_bytes / 1024 / 1024:.1f} MB total, ~{avg_size} bytes avg"
        )
        return metadata

    def prepare_cifar100(
        self, name: str = "cifar100", max_samples: int = 20000
    ) -> Dict:
        """
        Download CIFAR-100 and save images as JPEG at varying quality levels.

        This creates a variable-size dataset with realistic content.
        Quality varies from 50-95 to produce files from ~500B to ~3KB.
        """
        from torchvision.datasets import CIFAR100

        print(
            f"\n📦 Downloading CIFAR-100 ({max_samples} samples, variable JPEG quality)..."
        )
        dataset = CIFAR100(root=str(self._cache_dir), train=True, download=True)

        dataset_dir = self.raw_dir / name
        dataset_dir.mkdir(exist_ok=True)

        manifest = []
        total_bytes = 0
        num_samples = min(max_samples, len(dataset))

        # Vary JPEG quality to create variable file sizes
        import random

        rng = random.Random(42)

        for i in tqdm(range(num_samples), desc=f"  Saving {name}"):
            img, label = dataset[i]
            sample_path = dataset_dir / f"sample_{i:06d}.jpg"

            quality = rng.randint(50, 95)
            buf = io.BytesIO()
            img.save(buf, format="JPEG", quality=quality)
            data = buf.getvalue()
            with open(sample_path, "wb") as f:
                f.write(data)
            size = len(data)

            manifest.append(
                {
                    "id": i,
                    "path": str(sample_path.relative_to(self.output_dir)),
                    "size": size,
                    "label": int(label),
                }
            )
            total_bytes += size

        manifest_path = dataset_dir / "manifest.json"
        with open(manifest_path, "w") as f:
            json.dump(manifest, f, indent=2)

        avg_size = total_bytes // num_samples if num_samples > 0 else 0
        metadata = {
            "name": name,
            "num_samples": num_samples,
            "total_size": total_bytes,
            "avg_size": avg_size,
            "variable_size": True,
            "image_like": True,
            "manifest": str(manifest_path.relative_to(self.output_dir)),
            "source": "CIFAR-100 (torchvision), variable JPEG quality",
        }

        print(
            f"   ✓ {num_samples} images, {total_bytes / 1024 / 1024:.1f} MB total, ~{avg_size} bytes avg"
        )
        return metadata

    def prepare_tiny(self, name: str = "tiny", max_samples: int = 1000) -> Dict:
        """Small subset of CIFAR-10 for quick smoke tests."""
        return self.prepare_cifar10(name=name, max_samples=max_samples)

    def generate_all(self) -> Dict:
        """Download and prepare all benchmark datasets."""
        datasets = {}

        # 1. CIFAR-10: 50K small PNG images (~1-3KB each, ~80MB total)
        datasets["cifar10"] = self.prepare_cifar10()

        # 2. STL-10: 10K medium JPEG images (~5-15KB each, ~60-100MB total)
        datasets["stl10"] = self.prepare_stl10()

        # 3. CIFAR-100: 20K variable-quality JPEGs (~500B-3KB each, ~25MB total)
        datasets["cifar100"] = self.prepare_cifar100()

        # 4. Tiny: 1K images for quick tests
        datasets["tiny"] = self.prepare_tiny()

        # Save overall metadata
        metadata_path = self.output_dir / "datasets.json"
        with open(metadata_path, "w") as f:
            json.dump(datasets, f, indent=2)

        total_size = sum(d["total_size"] for d in datasets.values())
        total_samples = sum(d["num_samples"] for d in datasets.values())

        print("\n" + "=" * 60)
        print("Summary")
        print("=" * 60)
        print(f"Total datasets: {len(datasets)}")
        print(f"Total samples: {total_samples:,}")
        print(f"Total size: {total_size / 1024 / 1024:.1f} MB")
        print(f"Metadata: {metadata_path}")
        print("=" * 60)

        return datasets


def main():
    parser = argparse.ArgumentParser(
        description="Download real datasets for benchmarks"
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path(__file__).parent / "data",
        help="Output directory for test data",
    )
    parser.add_argument(
        "--quick",
        action="store_true",
        help="Download only tiny dataset for quick testing",
    )

    args = parser.parse_args()

    print("Real Dataset Downloader")
    print(f"Output directory: {args.output_dir}")

    generator = RealDataGenerator(args.output_dir)

    if args.quick:
        datasets = {"tiny": generator.prepare_tiny()}
        metadata_path = args.output_dir / "datasets.json"
        with open(metadata_path, "w") as f:
            json.dump(datasets, f, indent=2)
    else:
        generator.generate_all()

    print("\nTest data ready!")
    print("\nNext steps:")
    print("  1. Run benchmarks: python run_benchmarks.py --dataset all")
    print("  2. View results: cat results/*.json")


if __name__ == "__main__":
    main()
