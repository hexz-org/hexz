"""Example: LLM Weight Deduplication.

This example demonstrates how Hexz saves massive amounts of space when
storing multiple versions of a Large Language Model (e.g., base model
vs. multiple fine-tuned variants) by using content-defined chunking (CDC).
"""

import numpy as np
import hexz
from pathlib import Path


def create_model_weights(size_mb, noise_scale=0.0):
    """Simulate model weights as a large numpy array."""
    data = np.random.bytes(size_mb * 1024 * 1024)
    if noise_scale > 0:
        # In a real fine-tuning, most weights stay identical,
        # and some layers change. We simulate this by taking the base
        # and "tweaking" a small portion.
        pass
    return data


def run_example():
    # 1. Setup paths
    base_path = "model_base.hxz"
    variant_path = "model_finetuned.hxz"

    # Simulate a small 10MB model for the example
    model_size_mb = 10
    print(f"Generating {model_size_mb}MB base model weights...")
    base_weights = np.random.bytes(model_size_mb * 1024 * 1024)

    # 2. Save the Base Model
    print("Saving base model...")
    with hexz.Writer(base_path, packing="tight") as writer:
        writer.add(base_weights)

    # 3. Simulate Fine-tuning
    # We keep 90% of the weights the same and change 10%
    print("Simulating fine-tuning (changing 10% of weights)...")
    change_at = int(len(base_weights) * 0.9)
    finetuned_weights = base_weights[:change_at] + np.random.bytes(
        len(base_weights) - change_at
    )

    # 4. Save the Fine-tuned Model as a THIN snapshot
    # This is the key: we tell Hexz to only store the diffs
    # relative to the base model.
    print("Saving fine-tuned model as a thin snapshot...")

    # We write the delta to a temporary overlay file
    overlay_path = "model_delta.bin"
    with open(overlay_path, "wb") as f:
        f.write(finetuned_weights)

    # Now merge it into a thin snapshot using the Writer API
    with hexz.open(variant_path, mode="w") as writer:
        writer.merge_overlay(base=base_path, overlay=overlay_path, thin=True)

    # 5. Analyze Savings
    base_size = Path(base_path).stat().st_size
    thin_size = Path(variant_path).stat().st_size

    print("\nSpace Savings Comparison:")
    print(f"  Base Snapshot Size: {base_size / 1024:.1f} KB")
    print(f"  Thin Snapshot Size: {thin_size / 1024:.1f} KB")

    savings = (1 - (thin_size / base_size)) * 100
    print(f"  Total Storage Saved: {savings:.1f}%")

    # Clean up
    Path(base_path).unlink()
    Path(variant_path).unlink()
    Path(overlay_path).unlink()
    if Path("model_delta.meta").exists():
        Path("model_delta.meta").unlink()


if __name__ == "__main__":
    run_example()
