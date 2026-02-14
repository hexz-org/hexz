#!/usr/bin/env python3
"""
Generate synthetic test data for benchmarking.

Creates a dataset similar to ImageNet validation set:
- 50,000 images (224x224 RGB)
- ~6.3GB total size
- Mix of compressible patterns (simulates real images)
"""

import argparse
import json
from pathlib import Path
from typing import Dict, Any

import numpy as np
from PIL import Image
from tqdm import tqdm


def generate_synthetic_image(index: int, size: tuple = (224, 224)) -> np.ndarray:
    """
    Generate a synthetic image with compressible patterns.

    Simulates real images by mixing:
    - Smooth gradients (like sky, background)
    - Random noise (like texture, details)
    - Structured patterns (like edges, objects)
    """
    rng = np.random.RandomState(seed=index)

    # Create base gradient (smooth, highly compressible)
    y, x = np.mgrid[0 : size[0], 0 : size[1]]
    gradient = np.stack(
        [
            (x / size[1] * 255).astype(np.uint8),
            (y / size[0] * 255).astype(np.uint8),
            ((x + y) / (size[0] + size[1]) * 255).astype(np.uint8),
        ],
        axis=-1,
    )

    # Add some structured noise (medium compressibility)
    noise_scale = rng.randint(10, 50)
    noise = rng.randint(-noise_scale, noise_scale, size=size + (3,), dtype=np.int16)

    # Combine
    img = np.clip(gradient.astype(np.int16) + noise, 0, 255).astype(np.uint8)

    return img


def generate_dataset(
    output_dir: Path,
    num_images: int = 50000,
    image_size: tuple = (224, 224),
) -> Dict[str, Any]:
    """Generate synthetic dataset."""

    output_dir.mkdir(parents=True, exist_ok=True)

    print(f"Generating {num_images} images ({image_size[0]}x{image_size[1]})...")
    print(f"Output: {output_dir}")

    metadata = {
        "num_images": num_images,
        "image_size": image_size,
        "format": "JPEG",
        "total_bytes": 0,
    }

    total_bytes = 0

    for i in tqdm(range(num_images)):
        # Generate image
        img_array = generate_synthetic_image(i, image_size)
        img = Image.fromarray(img_array, mode="RGB")

        # Save as JPEG (realistic for ImageNet)
        img_path = output_dir / f"img_{i:06d}.jpg"
        img.save(img_path, format="JPEG", quality=85)

        total_bytes += img_path.stat().st_size

    metadata["total_bytes"] = total_bytes
    metadata["total_gb"] = total_bytes / (1024**3)

    # Save metadata
    metadata_path = output_dir / "metadata.json"
    with open(metadata_path, "w") as f:
        json.dump(metadata, f, indent=2)

    print("\nDataset generated:")
    print(f"  Images: {num_images}")
    print(f"  Total size: {metadata['total_gb']:.2f} GB")
    print(f"  Avg per image: {total_bytes / num_images / 1024:.1f} KB")

    return metadata


def main():
    parser = argparse.ArgumentParser(description="Generate synthetic test dataset")
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("benchmarks/data/imagenet_val_50k"),
        help="Output directory for generated images",
    )
    parser.add_argument(
        "--num-images",
        type=int,
        default=50000,
        help="Number of images to generate (default: 50000)",
    )
    parser.add_argument(
        "--image-size",
        type=int,
        default=224,
        help="Image size (square, default: 224)",
    )
    parser.add_argument(
        "--small",
        action="store_true",
        help="Generate small test set (1000 images) for quick testing",
    )

    args = parser.parse_args()

    if args.small:
        args.num_images = 1000
        args.output = Path("benchmarks/data/test_small")

    generate_dataset(
        args.output,
        num_images=args.num_images,
        image_size=(args.image_size, args.image_size),
    )


if __name__ == "__main__":
    main()
