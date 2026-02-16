#!/usr/bin/env python3
"""ML training workload for profiling hexz Python bindings.

Creates a synthetic dataset, packs it into a hexz snapshot, then runs a
PyTorch training loop to reveal data-loading vs compute bottlenecks.

Configure via environment variables:
    HEXZ_PERF_SAMPLES   Number of samples      (default: 10000)
    HEXZ_PERF_EPOCHS    Training epochs         (default: 3)
    HEXZ_PERF_BATCH     Batch size              (default: 64)
"""

import os
import tempfile
import time

import numpy as np

# ── Configuration ──────────────────────────────────────────────────────────────
NUM_SAMPLES = int(os.environ.get("HEXZ_PERF_SAMPLES", 10_000))
NUM_EPOCHS = int(os.environ.get("HEXZ_PERF_EPOCHS", 3))
BATCH_SIZE = int(os.environ.get("HEXZ_PERF_BATCH", 64))

CHANNELS, HEIGHT, WIDTH = 3, 64, 64
ITEM_SIZE = CHANNELS * HEIGHT * WIDTH  # 12 288 bytes per "image"


# ── Dataset generation ─────────────────────────────────────────────────────────
def generate_dataset(workdir: str) -> str:
    """Pack synthetic image data into a hexz snapshot."""
    import hexz

    raw_path = os.path.join(workdir, "raw.bin")
    snap_path = os.path.join(workdir, "dataset.hxz")

    rng = np.random.default_rng(42)
    data = rng.integers(0, 256, size=(NUM_SAMPLES * ITEM_SIZE,), dtype=np.uint8)

    with open(raw_path, "wb") as f:
        f.write(data.tobytes())

    with hexz.Writer(snap_path, compression="lz4") as w:
        w.add(raw_path)

    return snap_path


# ── Training loop ──────────────────────────────────────────────────────────────
def train(snap_path: str) -> None:
    """Run a classic CNN training loop over a hexz-backed dataset."""
    import torch
    import torch.nn as nn

    import hexz

    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")

    dataset = hexz.Dataset(
        snap_path,
        item_size=ITEM_SIZE,
        output_format="tensor",
        cache_size_mb=256,
        shuffle=True,
        seed=42,
    )

    loader = torch.utils.data.DataLoader(dataset, batch_size=BATCH_SIZE, num_workers=0)

    model = nn.Sequential(
        nn.Unflatten(1, (CHANNELS, HEIGHT, WIDTH)),
        nn.Conv2d(CHANNELS, 32, 3, padding=1),
        nn.ReLU(),
        nn.MaxPool2d(2),
        nn.Conv2d(32, 64, 3, padding=1),
        nn.ReLU(),
        nn.AdaptiveAvgPool2d(4),
        nn.Flatten(),
        nn.Linear(64 * 4 * 4, 10),
    ).to(device)

    optimizer = torch.optim.SGD(model.parameters(), lr=0.01, momentum=0.9)
    criterion = nn.CrossEntropyLoss()

    for epoch in range(NUM_EPOCHS):
        dataset.set_epoch(epoch)
        total_loss = 0.0
        n = 0

        for batch in loader:
            x = batch.float().to(device) / 255.0
            y = torch.randint(0, 10, (x.shape[0],), device=device)

            out = model(x)
            loss = criterion(out, y)

            optimizer.zero_grad()
            loss.backward()
            optimizer.step()

            total_loss += loss.item()
            n += 1

        print(f"  epoch {epoch + 1}/{NUM_EPOCHS}  loss={total_loss / n:.4f}")


# ── Entry point ────────────────────────────────────────────────────────────────
def main():
    print(f"hexz perf: {NUM_SAMPLES} samples, {NUM_EPOCHS} epochs, batch={BATCH_SIZE}")

    with tempfile.TemporaryDirectory(prefix="hexz_perf_") as workdir:
        t0 = time.monotonic()
        snap = generate_dataset(workdir)
        dt = time.monotonic() - t0
        print(f"  dataset generated in {dt:.1f}s")

        t0 = time.monotonic()
        train(snap)
        dt = time.monotonic() - t0
        print(f"  training completed in {dt:.1f}s")


if __name__ == "__main__":
    main()
