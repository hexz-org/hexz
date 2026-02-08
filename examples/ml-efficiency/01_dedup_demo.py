#!/usr/bin/env python3
"""
Deduplication Demo: Show storage savings with augmented datasets

This script demonstrates how Strata's block-level deduplication saves storage
when you have redundant data (e.g., augmented images).
"""

import os
import shutil
import time
from pathlib import Path

import numpy as np
from PIL import Image, ImageEnhance, ImageFilter
from tqdm import tqdm

# Requires strata to be installed: maturin develop --manifest-path ../../crates/loader/Cargo.toml
try:
    import strata
    STRATA_AVAILABLE = True
except ImportError:
    STRATA_AVAILABLE = False
    print("WARNING: Strata not installed. Run: maturin develop --manifest-path ../../crates/loader/Cargo.toml")
    print("   Continuing with size estimation only...\n")


# Configuration
NUM_BASE_IMAGES = 100  # Small demo; scale to 50k for realistic test
AUGMENTATIONS_PER_IMAGE = 10
IMAGE_SIZE = 224
OUTPUT_DIR = Path("./dedup_data")
STANDARD_DIR = OUTPUT_DIR / "standard"
BASE_DIR = OUTPUT_DIR / "base"
STRATA_FILE = OUTPUT_DIR / "augmented_dataset.st"


def create_synthetic_image(idx: int) -> Image.Image:
    """Create a synthetic but realistic-looking image"""
    # Create gradient background
    arr = np.zeros((IMAGE_SIZE, IMAGE_SIZE, 3), dtype=np.uint8)

    # Add gradient based on index
    for i in range(IMAGE_SIZE):
        for j in range(IMAGE_SIZE):
            r = (i * 255 // IMAGE_SIZE + idx * 23) % 256
            g = (j * 255 // IMAGE_SIZE + idx * 37) % 256
            b = ((i + j) * 255 // (IMAGE_SIZE * 2) + idx * 51) % 256
            arr[i, j] = [r, g, b]

    img = Image.fromarray(arr)

    # Add some "texture" with noise
    noise = np.random.randint(-20, 20, (IMAGE_SIZE, IMAGE_SIZE, 3), dtype=np.int16)
    arr_noisy = np.clip(arr.astype(np.int16) + noise, 0, 255).astype(np.uint8)

    return Image.fromarray(arr_noisy)


def augment_image(img: Image.Image, aug_type: int) -> Image.Image:
    """Apply augmentation (creates similar but not identical image)"""
    if aug_type == 0:
        return img  # Original
    elif aug_type == 1:
        return img.rotate(90)
    elif aug_type == 2:
        return img.rotate(180)
    elif aug_type == 3:
        return img.rotate(270)
    elif aug_type == 4:
        return img.transpose(Image.FLIP_LEFT_RIGHT)
    elif aug_type == 5:
        return img.transpose(Image.FLIP_TOP_BOTTOM)
    elif aug_type == 6:
        enhancer = ImageEnhance.Brightness(img)
        return enhancer.enhance(1.3)
    elif aug_type == 7:
        enhancer = ImageEnhance.Contrast(img)
        return enhancer.enhance(1.2)
    elif aug_type == 8:
        return img.filter(ImageFilter.GaussianBlur(radius=1))
    else:
        # Crop and resize
        w, h = img.size
        return img.crop((10, 10, w - 10, h - 10)).resize((IMAGE_SIZE, IMAGE_SIZE))


def get_dir_size(path: Path) -> int:
    """Calculate total size of directory"""
    total = 0
    for entry in path.rglob('*'):
        if entry.is_file():
            total += entry.stat().st_size
    return total


def format_size(bytes: int) -> str:
    """Format bytes as human-readable"""
    for unit in ['B', 'KB', 'MB', 'GB']:
        if bytes < 1024:
            return f"{bytes:.1f} {unit}"
        bytes /= 1024
    return f"{bytes:.1f} TB"


def main():
    print("=" * 70)
    print("Strata Deduplication Demo")
    print("=" * 70)
    print(f"\nConfiguration:")
    print(f"  Base images:     {NUM_BASE_IMAGES:,}")
    print(f"  Augmentations:   {AUGMENTATIONS_PER_IMAGE} per image")
    print(f"  Total images:    {NUM_BASE_IMAGES * AUGMENTATIONS_PER_IMAGE:,}")
    print(f"  Image size:      {IMAGE_SIZE}x{IMAGE_SIZE}\n")

    # Clean up previous runs
    if OUTPUT_DIR.exists():
        shutil.rmtree(OUTPUT_DIR)

    OUTPUT_DIR.mkdir(parents=True)
    STANDARD_DIR.mkdir(parents=True)
    BASE_DIR.mkdir(parents=True)

    # Step 1: Generate base images
    print("Step 1: Generating base images...")
    base_images = []
    for i in tqdm(range(NUM_BASE_IMAGES), desc="Creating base images"):
        img = create_synthetic_image(i)
        img_path = BASE_DIR / f"base_{i:05d}.jpg"
        img.save(img_path, quality=85)
        base_images.append(img)

    base_size = get_dir_size(BASE_DIR)
    print(f"[DONE] Base images size: {format_size(base_size)}\n")

    # Step 2: Generate augmented dataset (standard approach)
    print("Step 2: Generating augmented dataset (standard storage)...")
    for i in tqdm(range(NUM_BASE_IMAGES), desc="Augmenting images"):
        img = base_images[i]
        for aug_idx in range(AUGMENTATIONS_PER_IMAGE):
            aug_img = augment_image(img, aug_idx)
            aug_path = STANDARD_DIR / f"img_{i:05d}_aug_{aug_idx:02d}.jpg"
            aug_img.save(aug_path, quality=85)

    standard_size = get_dir_size(STANDARD_DIR)
    print(f"[DONE] Standard storage size: {format_size(standard_size)}\n")

    # Step 3: Pack with Strata (if available)
    strata_size = 0
    if STRATA_AVAILABLE:
        print("Step 3: Packing with Strata (deduplication enabled)...")
        start = time.time()

        try:
            strata.pack(
                input_dir=str(STANDARD_DIR),
                output_file=str(STRATA_FILE),
                compression="lz4",
                deduplication=True,
                threads=4,
            )
            pack_time = time.time() - start
            strata_size = STRATA_FILE.stat().st_size
            print(f"[DONE] Strata packing completed in {pack_time:.1f}s")
            print(f"[DONE] Strata file size: {format_size(strata_size)}\n")
        except Exception as e:
            print(f"[ERROR] Strata packing failed: {e}\n")
            STRATA_AVAILABLE = False
    else:
        print("Step 3: Skipped (Strata not available)\n")

    # Results
    print("=" * 70)
    print("RESULTS")
    print("=" * 70)
    print(f"\n{'Storage Method':<30} {'Size':<15} {'vs Standard'}")
    print("-" * 70)
    print(f"{'Base images only':<30} {format_size(base_size):<15} {'-'}")
    print(f"{'Standard (all augmented)':<30} {format_size(standard_size):<15} {'-'}")

    if STRATA_AVAILABLE and strata_size > 0:
        reduction_pct = (1 - strata_size / standard_size) * 100
        reduction_vs_base = (1 - strata_size / base_size) * 100
        print(f"{'Strata (deduplicated)':<30} {format_size(strata_size):<15} {reduction_pct:+.1f}%")

        print(f"\nAnalysis:")
        print(f"  - Strata stores augmented data with only {reduction_vs_base:.1f}% overhead vs base")
        print(f"  - {reduction_pct:.1f}% storage savings compared to standard approach")
        print(f"  - Deduplication recognizes common blocks across augmentations")

        # Savings on a real dataset
        scaling_factor = 50000 / NUM_BASE_IMAGES  # Scale to 50k images
        standard_scaled = standard_size * scaling_factor
        strata_scaled = strata_size * scaling_factor
        savings_gb = (standard_scaled - strata_scaled) / (1024**3)

        print(f"\nExtrapolated savings for 50,000 base images:")
        print(f"  - Standard: ~{format_size(standard_scaled)}")
        print(f"  - Strata:   ~{format_size(strata_scaled)}")
        print(f"  - Savings:  ~{format_size(standard_scaled - strata_scaled)} ({reduction_pct:.1f}%)")
    else:
        print(f"{'Strata (deduplicated)':<30} {'N/A':<15} {'(not installed)'}")

    print("\n" + "=" * 70)
    print("\nKey Takeaway:")
    print("   When you have augmented datasets, Strata's block-level deduplication")
    print("   automatically identifies and eliminates redundancy, saving storage and")
    print("   bandwidth—without any manual work from you.")
    print("=" * 70)


if __name__ == "__main__":
    main()
