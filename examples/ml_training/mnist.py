#!/usr/bin/env python3
"""Train a CNN on MNIST packed into a hexz snapshot.

Downloads MNIST via torchvision, packs images+labels into hexz format,
then trains a small CNN using hexz.Dataset + PyTorch DataLoader.

Usage:
    python examples/ml_training/mnist.py
"""

import os
import struct
import tempfile
import time

import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F
from torchvision import datasets

import hexz

# ── Configuration ──────────────────────────────────────────────────────────────
BATCH_SIZE = 128
EPOCHS = 5
LR = 0.01
ITEM_SIZE = 1 + 28 * 28  # 1 label byte + 784 pixel bytes = 785


# ── Helpers ────────────────────────────────────────────────────────────────────
def fmt_bytes(n: float) -> str:
    for unit in ("B", "KB", "MB", "GB"):
        if abs(n) < 1024:
            return f"{n:.1f} {unit}"
        n /= 1024
    return f"{n:.1f} TB"


def fmt_rate(bytes_per_sec: float) -> str:
    return f"{fmt_bytes(bytes_per_sec)}/s"


def separator(title: str = "") -> str:
    if title:
        return f"\n{'─' * 3} {title} {'─' * (54 - len(title))}"
    return "─" * 60


# ── Model ──────────────────────────────────────────────────────────────────────
class MNISTNet(nn.Module):
    def __init__(self):
        super().__init__()
        self.conv1 = nn.Conv2d(1, 32, 3, padding=1)
        self.conv2 = nn.Conv2d(32, 64, 3, padding=1)
        self.pool = nn.MaxPool2d(2)
        self.fc1 = nn.Linear(64 * 7 * 7, 128)
        self.fc2 = nn.Linear(128, 10)

    def forward(self, x):
        x = self.pool(F.relu(self.conv1(x)))
        x = self.pool(F.relu(self.conv2(x)))
        x = x.view(-1, 64 * 7 * 7)
        x = F.relu(self.fc1(x))
        return self.fc2(x)


# ── Pack MNIST into hexz ──────────────────────────────────────────────────────
def pack_mnist(workdir: str) -> tuple[str, str, dict]:
    """Download MNIST and pack train/test sets into hexz snapshots."""
    print("Downloading MNIST...")
    train_ds = datasets.MNIST(workdir, train=True, download=True)
    test_ds = datasets.MNIST(workdir, train=False, download=True)

    train_path = os.path.join(workdir, "mnist_train.hxz")
    test_path = os.path.join(workdir, "mnist_test.hxz")

    stats = {}

    for ds, path, name in [
        (train_ds, train_path, "train"),
        (test_ds, test_path, "test"),
    ]:
        raw_path = os.path.join(workdir, f"{name}.bin")

        t0 = time.monotonic()
        with open(raw_path, "wb") as f:
            for img, label in ds:
                f.write(struct.pack("B", label))
                f.write(np.array(img, dtype=np.uint8).tobytes())

        raw_size = os.path.getsize(raw_path)

        with hexz.Writer(path, compression="lz4") as w:
            w.add(raw_path)

        pack_time = time.monotonic() - t0
        os.unlink(raw_path)

        compressed_size = os.path.getsize(path)
        meta = hexz.inspect(path)

        stats[name] = {
            "samples": len(ds),
            "raw_size": raw_size,
            "compressed_size": compressed_size,
            "pack_time": pack_time,
            "meta": meta,
        }

    return train_path, test_path, stats


def print_pack_stats(stats: dict):
    """Print detailed packing and compression statistics."""
    print(separator("Snapshot Stats"))

    for name in ("train", "test"):
        s = stats[name]
        meta = s["meta"]
        raw = s["raw_size"]
        comp = s["compressed_size"]
        ratio = raw / comp if comp > 0 else 0
        saving = (1 - comp / raw) * 100 if raw > 0 else 0
        throughput = raw / s["pack_time"] if s["pack_time"] > 0 else 0

        print(f"\n  {name.upper()} set:")
        print(f"    Samples:          {s['samples']:,}")
        print(f"    Item size:        {ITEM_SIZE} bytes ({28}x{28} + label)")
        print(f"    Raw size:         {fmt_bytes(raw)}")
        print(f"    Compressed size:  {fmt_bytes(comp)}")
        print(f"    Ratio:            {ratio:.2f}x  ({saving:.1f}% smaller)")
        print(f"    Pack time:        {s['pack_time']:.3f}s")
        print(f"    Pack throughput:  {fmt_rate(throughput)}")
        print(f"    Compression:      {meta.compression}")
        print(f"    Block size:       {fmt_bytes(meta.block_size)}")
        print(f"    Blocks:           {meta.num_blocks:,}")
        print(f"    Format version:   {meta.version}")
        print(f"    Encrypted:        {'yes' if meta.encrypted else 'no'}")
        print(f"    Signed:           {'yes' if meta.signed else 'no'}")

    # Combined totals
    total_raw = sum(s["raw_size"] for s in stats.values())
    total_comp = sum(s["compressed_size"] for s in stats.values())
    total_time = sum(s["pack_time"] for s in stats.values())
    total_samples = sum(s["samples"] for s in stats.values())
    print("\n  TOTAL:")
    print(
        f"    {total_samples:,} samples, {fmt_bytes(total_raw)} raw → {fmt_bytes(total_comp)} compressed"
    )
    print(f"    {total_raw / total_comp:.2f}x ratio, packed in {total_time:.3f}s")


# ── Training ───────────────────────────────────────────────────────────────────
def train(train_path: str, test_path: str) -> dict:
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")

    train_dataset = hexz.Dataset(
        train_path,
        item_size=ITEM_SIZE,
        output_format="tensor",
        cache_size_mb=256,
        shuffle=True,
        seed=42,
    )
    test_dataset = hexz.Dataset(
        test_path,
        item_size=ITEM_SIZE,
        output_format="tensor",
        cache_size_mb=128,
    )

    train_loader = torch.utils.data.DataLoader(
        train_dataset, batch_size=BATCH_SIZE, num_workers=0
    )
    test_loader = torch.utils.data.DataLoader(
        test_dataset, batch_size=BATCH_SIZE, num_workers=0
    )

    model = MNISTNet().to(device)
    param_count = sum(p.numel() for p in model.parameters())
    optimizer = torch.optim.SGD(model.parameters(), lr=LR, momentum=0.9)
    criterion = nn.CrossEntropyLoss()

    print(separator("Training"))
    print(f"\n  Device:       {device}")
    print(f"  Model:        MNISTNet ({param_count:,} params)")
    print(f"  Optimizer:    SGD (lr={LR}, momentum=0.9)")
    print(f"  Batch size:   {BATCH_SIZE}")
    print(f"  Train set:    {len(train_dataset):,} samples")
    print(f"  Test set:     {len(test_dataset):,} samples")
    print()

    epoch_stats = []
    total_t0 = time.monotonic()

    for epoch in range(EPOCHS):
        model.train()
        train_dataset.set_epoch(epoch)
        total_loss = 0.0
        correct = 0
        total = 0
        data_time = 0.0
        compute_time = 0.0

        t_epoch = time.monotonic()
        t_data = time.monotonic()

        for batch in train_loader:
            data_time += time.monotonic() - t_data

            t_compute = time.monotonic()
            raw = batch.numpy()
            labels = torch.from_numpy(raw[:, 0].astype(np.int64)).to(device)
            pixels = (
                torch.from_numpy(raw[:, 1:].astype(np.float32) / 255.0)
                .view(-1, 1, 28, 28)
                .to(device)
            )

            out = model(pixels)
            loss = criterion(out, labels)

            optimizer.zero_grad()
            loss.backward()
            optimizer.step()

            total_loss += loss.item() * len(labels)
            correct += (out.argmax(1) == labels).sum().item()
            total += len(labels)
            compute_time += time.monotonic() - t_compute

            t_data = time.monotonic()

        dt = time.monotonic() - t_epoch
        acc = 100 * correct / total
        throughput = total / dt
        data_pct = 100 * data_time / dt if dt > 0 else 0

        epoch_stats.append(
            {
                "loss": total_loss / total,
                "acc": acc,
                "time": dt,
                "throughput": throughput,
                "data_time": data_time,
                "compute_time": compute_time,
                "data_pct": data_pct,
            }
        )

        print(
            f"  epoch {epoch + 1}/{EPOCHS}  "
            f"loss={total_loss / total:.4f}  "
            f"acc={acc:.1f}%  "
            f"{throughput:.0f} samples/s  "
            f"({dt:.1f}s, data={data_pct:.0f}%)"
        )

    total_train_time = time.monotonic() - total_t0

    # ── Test ───────────────────────────────────────────────────────────────────
    model.eval()
    correct = 0
    total = 0
    t0 = time.monotonic()
    with torch.no_grad():
        for batch in test_loader:
            raw = batch.numpy()
            labels = torch.from_numpy(raw[:, 0].astype(np.int64)).to(device)
            pixels = (
                torch.from_numpy(raw[:, 1:].astype(np.float32) / 255.0)
                .view(-1, 1, 28, 28)
                .to(device)
            )
            out = model(pixels)
            correct += (out.argmax(1) == labels).sum().item()
            total += len(labels)

    test_time = time.monotonic() - t0
    test_acc = 100 * correct / total

    train_cache = train_dataset.cache_stats()
    test_cache = test_dataset.cache_stats()

    return {
        "epochs": epoch_stats,
        "total_train_time": total_train_time,
        "test_acc": test_acc,
        "test_correct": correct,
        "test_total": total,
        "test_time": test_time,
        "train_cache": train_cache,
        "test_cache": test_cache,
    }


def print_train_stats(results: dict):
    """Print detailed training and cache statistics."""
    print(separator("Results"))

    test_acc = results["test_acc"]
    print(
        f"\n  Test accuracy:    {test_acc:.1f}% ({results['test_correct']}/{results['test_total']})"
    )
    print(f"  Test eval time:   {results['test_time']:.2f}s")
    print(f"  Total train time: {results['total_train_time']:.2f}s")

    # Average epoch stats
    epochs = results["epochs"]
    avg_throughput = sum(e["throughput"] for e in epochs) / len(epochs)
    avg_data_pct = sum(e["data_pct"] for e in epochs) / len(epochs)
    total_data_time = sum(e["data_time"] for e in epochs)
    total_compute_time = sum(e["compute_time"] for e in epochs)

    print(f"\n  Avg throughput:   {avg_throughput:.0f} samples/s")
    print(
        f"  Data loading:     {total_data_time:.2f}s ({avg_data_pct:.1f}% of epoch time)"
    )
    print(
        f"  Compute:          {total_compute_time:.2f}s ({100 - avg_data_pct:.1f}% of epoch time)"
    )

    print(separator("Cache Stats"))

    for name, cache in [
        ("train", results["train_cache"]),
        ("test", results["test_cache"]),
    ]:
        if not cache.get("enabled"):
            print(f"\n  {name}: disabled")
            continue
        total_ops = cache["hits"] + cache["misses"]
        print(f"\n  {name.upper()} cache:")
        print(f"    Hit rate:   {100 * cache['hit_rate']:.1f}%")
        print(f"    Hits:       {cache['hits']:,}")
        print(f"    Misses:     {cache['misses']:,}")
        print(f"    Total ops:  {total_ops:,}")
        print(f"    Size:       {cache['size_mb']:.1f} MB")
        print(f"    Items:      {cache['items']:,}")


# ── Main ───────────────────────────────────────────────────────────────────────
def main():
    print("=" * 60)
    print("  MNIST + Hexz — End-to-End ML Training Demo")
    print("=" * 60)

    with tempfile.TemporaryDirectory(prefix="hexz_mnist_") as workdir:
        train_path, test_path, pack_stats = pack_mnist(workdir)
        print_pack_stats(pack_stats)

        results = train(train_path, test_path)
        print_train_stats(results)

    print(f"\n{'=' * 60}")


if __name__ == "__main__":
    main()
