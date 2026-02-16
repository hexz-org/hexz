#!/usr/bin/env python3
"""CIFAR-10 Benchmark: Hexz vs Native PyTorch DataLoader.

Same model, same hyperparameters, same data — only the data loading
changes.  Shows storage savings and training throughput side-by-side.

Usage:
    python examples/ml_training/cifar10_benchmark.py
    python examples/ml_training/cifar10_benchmark.py --epochs 30
"""

import argparse
import os
import struct
import tempfile
import time

import numpy as np
import torch
import torch.nn as nn
from torchvision import datasets, transforms

import hexz

# ── Configuration ──────────────────────────────────────────────────────────────
BATCH_SIZE = 128
LR = 0.01
SEED = 42
ITEM_SIZE = 1 + 3 * 32 * 32  # 1 label + 3072 pixels = 3073 bytes


# ── Helpers ────────────────────────────────────────────────────────────────────
def fmt_bytes(n: float) -> str:
    for unit in ("B", "KB", "MB", "GB"):
        if abs(n) < 1024:
            return f"{n:.1f} {unit}"
        n /= 1024
    return f"{n:.1f} TB"


def dir_size(path: str) -> int:
    total = 0
    for dirpath, _, filenames in os.walk(path):
        for f in filenames:
            total += os.path.getsize(os.path.join(dirpath, f))
    return total


def sep(title: str) -> str:
    return f"\n{'─' * 3} {title} {'─' * max(1, 56 - len(title))}"


# ── Model (shared by both runs) ───────────────────────────────────────────────
class CIFAR10Net(nn.Module):
    """VGG-style CNN for CIFAR-10 (~1.5M params)."""

    def __init__(self):
        super().__init__()
        self.features = nn.Sequential(
            nn.Conv2d(3, 64, 3, padding=1),
            nn.BatchNorm2d(64),
            nn.ReLU(),
            nn.Conv2d(64, 64, 3, padding=1),
            nn.BatchNorm2d(64),
            nn.ReLU(),
            nn.MaxPool2d(2),
            nn.Conv2d(64, 128, 3, padding=1),
            nn.BatchNorm2d(128),
            nn.ReLU(),
            nn.Conv2d(128, 128, 3, padding=1),
            nn.BatchNorm2d(128),
            nn.ReLU(),
            nn.MaxPool2d(2),
            nn.Conv2d(128, 256, 3, padding=1),
            nn.BatchNorm2d(256),
            nn.ReLU(),
            nn.AdaptiveAvgPool2d(4),
        )
        self.classifier = nn.Sequential(
            nn.Linear(256 * 4 * 4, 512),
            nn.ReLU(),
            nn.Dropout(0.5),
            nn.Linear(512, 10),
        )

    def forward(self, x):
        x = self.features(x)
        x = x.view(x.size(0), -1)
        return self.classifier(x)


# ── Training loop (shared) ────────────────────────────────────────────────────
def run_training(
    train_iter,
    test_iter,
    *,
    decode_fn,
    epochs: int,
    device: torch.device,
    label: str,
    set_epoch_fn=None,
):
    """Train and evaluate, return results dict.

    decode_fn(batch) -> (images: Tensor[B,3,32,32], labels: Tensor[B])
    """
    torch.manual_seed(SEED)
    model = CIFAR10Net().to(device)
    optimizer = torch.optim.SGD(
        model.parameters(), lr=LR, momentum=0.9, weight_decay=1e-4
    )
    scheduler = torch.optim.lr_scheduler.CosineAnnealingLR(optimizer, T_max=epochs)
    criterion = nn.CrossEntropyLoss()

    param_count = sum(p.numel() for p in model.parameters())
    print(f"\n  [{label}] {param_count:,} params, {epochs} epochs, batch={BATCH_SIZE}")

    epoch_stats = []
    t_total = time.monotonic()

    for epoch in range(epochs):
        if set_epoch_fn:
            set_epoch_fn(epoch)

        model.train()
        total_loss = 0.0
        correct = 0
        total = 0
        data_time = 0.0
        t_data = time.monotonic()

        for batch in train_iter:
            data_time += time.monotonic() - t_data

            images, labels = decode_fn(batch)
            images, labels = images.to(device), labels.to(device)

            out = model(images)
            loss = criterion(out, labels)

            optimizer.zero_grad()
            loss.backward()
            optimizer.step()

            total_loss += loss.item() * labels.size(0)
            correct += (out.argmax(1) == labels).sum().item()
            total += labels.size(0)

            t_data = time.monotonic()

        scheduler.step()
        dt_epoch = (
            (time.monotonic() - t_total) - sum(e["time"] for e in epoch_stats)
            if epoch_stats
            else time.monotonic() - t_total
        )
        data_pct = 100 * data_time / dt_epoch if dt_epoch > 0 else 0

        epoch_stats.append(
            {
                "loss": total_loss / total,
                "acc": 100 * correct / total,
                "time": dt_epoch,
                "throughput": total / dt_epoch if dt_epoch > 0 else 0,
                "data_time": data_time,
                "data_pct": data_pct,
            }
        )

        e = epoch_stats[-1]
        if epoch < 3 or epoch == epochs - 1 or (epoch + 1) % 5 == 0:
            print(
                f"  epoch {epoch + 1:>2}/{epochs}  "
                f"loss={e['loss']:.4f}  acc={e['acc']:.1f}%  "
                f"{e['throughput']:.0f} s/s  "
                f"({e['time']:.1f}s, data={e['data_pct']:.0f}%)"
            )

    total_time = time.monotonic() - t_total

    # Test
    model.eval()
    correct = 0
    total = 0
    with torch.no_grad():
        for batch in test_iter:
            images, labels = decode_fn(batch)
            images, labels = images.to(device), labels.to(device)
            out = model(images)
            correct += (out.argmax(1) == labels).sum().item()
            total += labels.size(0)

    test_acc = 100 * correct / total
    avg_throughput = sum(e["throughput"] for e in epoch_stats) / len(epoch_stats)
    avg_data_pct = sum(e["data_pct"] for e in epoch_stats) / len(epoch_stats)
    total_data = sum(e["data_time"] for e in epoch_stats)

    print(
        f"  test acc={test_acc:.1f}%  total={total_time:.1f}s  avg={avg_throughput:.0f} s/s"
    )

    return {
        "label": label,
        "test_acc": test_acc,
        "total_time": total_time,
        "avg_throughput": avg_throughput,
        "avg_data_pct": avg_data_pct,
        "total_data_time": total_data,
        "epochs": epoch_stats,
    }


# ── Native PyTorch ─────────────────────────────────────────────────────────────
def run_native(root: str, epochs: int, device: torch.device) -> dict:
    transform = transforms.ToTensor()  # PIL → [0,1] float32 tensor
    train_ds = datasets.CIFAR10(root, train=True, transform=transform)
    test_ds = datasets.CIFAR10(root, train=False, transform=transform)

    train_loader = torch.utils.data.DataLoader(
        train_ds, batch_size=BATCH_SIZE, shuffle=True, num_workers=0
    )
    test_loader = torch.utils.data.DataLoader(
        test_ds, batch_size=BATCH_SIZE, shuffle=False, num_workers=0
    )

    def decode(batch):
        images, labels = batch
        return images, labels

    return run_training(
        train_loader,
        test_loader,
        decode_fn=decode,
        epochs=epochs,
        device=device,
        label="Native",
    )


# ── Hexz ───────────────────────────────────────────────────────────────────────
def pack_cifar10(root: str, workdir: str) -> tuple[str, str, dict]:
    """Pack CIFAR-10 into hexz snapshots. Returns (train_path, test_path, stats)."""
    train_ds = datasets.CIFAR10(root, train=True)
    test_ds = datasets.CIFAR10(root, train=False)

    stats = {}

    for ds, name in [(train_ds, "train"), (test_ds, "test")]:
        raw_path = os.path.join(workdir, f"{name}.bin")
        snap_path = os.path.join(workdir, f"cifar10_{name}.hxz")

        t0 = time.monotonic()
        with open(raw_path, "wb") as f:
            for img, label in ds:
                f.write(struct.pack("B", label))
                f.write(np.array(img, dtype=np.uint8).transpose(2, 0, 1).tobytes())

        raw_size = os.path.getsize(raw_path)

        with hexz.Writer(snap_path, compression="lz4") as w:
            w.add(raw_path)

        pack_time = time.monotonic() - t0
        os.unlink(raw_path)
        comp_size = os.path.getsize(snap_path)

        stats[name] = {
            "samples": len(ds),
            "raw_size": raw_size,
            "compressed_size": comp_size,
            "pack_time": pack_time,
            "path": snap_path,
        }

    return stats["train"]["path"], stats["test"]["path"], stats


def _cifar10_transform(item: torch.Tensor):
    """Decode a single CIFAR-10 item from raw uint8 bytes into (image, label)."""
    label = item[0].long()
    image = item[1:].float().div_(255.0).view(3, 32, 32)
    return image, label


def run_hexz(
    train_path: str, test_path: str, epochs: int, device: torch.device
) -> dict:
    train_ds = hexz.Dataset(
        train_path,
        item_size=ITEM_SIZE,
        output_format="tensor",
        cache_size_mb=512,
        shuffle=True,
        seed=SEED,
        transform=_cifar10_transform,
    )
    test_ds = hexz.Dataset(
        test_path,
        item_size=ITEM_SIZE,
        output_format="tensor",
        cache_size_mb=128,
        transform=_cifar10_transform,
    )

    train_loader = torch.utils.data.DataLoader(
        train_ds, batch_size=BATCH_SIZE, num_workers=0
    )
    test_loader = torch.utils.data.DataLoader(
        test_ds, batch_size=BATCH_SIZE, num_workers=0
    )

    def decode(batch):
        images, labels = batch
        return images, labels

    result = run_training(
        train_loader,
        test_loader,
        decode_fn=decode,
        epochs=epochs,
        device=device,
        label="Hexz",
        set_epoch_fn=train_ds.set_epoch,
    )

    result["train_cache"] = train_ds.cache_stats()
    result["test_cache"] = test_ds.cache_stats()
    return result


# ── Comparison ─────────────────────────────────────────────────────────────────
def print_comparison(native: dict, hxz: dict, storage: dict):
    print(sep("Comparison"))

    # Storage
    native_size = storage["native_size"]
    hexz_size = sum(s["compressed_size"] for s in storage["hexz"].values())
    raw_size = sum(s["raw_size"] for s in storage["hexz"].values())
    hexz_pack_time = sum(s["pack_time"] for s in storage["hexz"].values())
    ratio = raw_size / hexz_size if hexz_size > 0 else 0
    saving = (1 - hexz_size / native_size) * 100 if native_size > 0 else 0

    print(f"\n  {'':30s} {'Native':>12s} {'Hexz':>12s} {'Delta':>10s}")
    print(f"  {'─' * 30} {'─' * 12} {'─' * 12} {'─' * 10}")

    print(
        f"  {'Disk usage':30s} {fmt_bytes(native_size):>12s} {fmt_bytes(hexz_size):>12s} {saving:>+9.1f}%"
    )
    print(f"  {'Compression ratio':30s} {'1.00x':>12s} {ratio:>11.2f}x {'':>10s}")
    print(f"  {'Pack time':30s} {'n/a':>12s} {hexz_pack_time:>11.2f}s {'':>10s}")

    # Training
    t_n = native["total_time"]
    t_h = hxz["total_time"]
    dt_time = (t_h - t_n) / t_n * 100

    tp_n = native["avg_throughput"]
    tp_h = hxz["avg_throughput"]
    dt_tp = (tp_h - tp_n) / tp_n * 100

    dp_n = native["avg_data_pct"]
    dp_h = hxz["avg_data_pct"]

    acc_n = native["test_acc"]
    acc_h = hxz["test_acc"]

    print(f"  {'Training time':30s} {t_n:>11.1f}s {t_h:>11.1f}s {dt_time:>+9.1f}%")
    print(
        f"  {'Avg throughput (samples/s)':30s} {tp_n:>12.0f} {tp_h:>12.0f} {dt_tp:>+9.1f}%"
    )
    print(
        f"  {'Data loading (% of epoch)':30s} {dp_n:>11.1f}% {dp_h:>11.1f}% {'':>10s}"
    )
    print(
        f"  {'Test accuracy':30s} {acc_n:>11.1f}% {acc_h:>11.1f}% {acc_h - acc_n:>+8.1f}pp"
    )

    # Cache
    tc = hxz.get("train_cache", {})
    if tc.get("enabled"):
        print(
            f"\n  Hexz cache: {100 * tc['hit_rate']:.0f}% hit rate, "
            f"{tc['hits']:,} hits / {tc['misses']:,} misses, "
            f"{tc['size_mb']:.1f} MB cached"
        )


# ── Main ───────────────────────────────────────────────────────────────────────
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--epochs", type=int, default=20, help="Training epochs (default: 20)"
    )
    args = ap.parse_args()

    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")

    print("=" * 60)
    print("  CIFAR-10 Benchmark: Hexz vs Native PyTorch")
    print("=" * 60)
    print(f"\n  Device: {device}")
    print(f"  Epochs: {args.epochs}")
    print(f"  Batch:  {BATCH_SIZE}")

    with tempfile.TemporaryDirectory(prefix="hexz_cifar_") as workdir:
        # Download once
        print(sep("Downloading CIFAR-10"))
        data_root = os.path.join(workdir, "data")
        datasets.CIFAR10(data_root, train=True, download=True)
        datasets.CIFAR10(data_root, train=False, download=True)

        # Measure native storage
        cifar_dir = os.path.join(data_root, "cifar-10-batches-py")
        native_size = dir_size(cifar_dir)
        print(f"\n  Native storage (pickle): {fmt_bytes(native_size)}")

        # Pack into hexz
        print(sep("Packing into Hexz"))
        train_path, test_path, hexz_stats = pack_cifar10(data_root, workdir)

        for name in ("train", "test"):
            s = hexz_stats[name]
            r = s["raw_size"] / s["compressed_size"]
            print(
                f"  {name}: {s['samples']:,} samples, "
                f"{fmt_bytes(s['raw_size'])} → {fmt_bytes(s['compressed_size'])} "
                f"({r:.2f}x, {s['pack_time']:.2f}s)"
            )

        storage = {"native_size": native_size, "hexz": hexz_stats}

        # Run native
        print(sep("Native PyTorch Training"))
        native_result = run_native(data_root, args.epochs, device)

        # Run hexz
        print(sep("Hexz Training"))
        hexz_result = run_hexz(train_path, test_path, args.epochs, device)

        # Comparison
        print_comparison(native_result, hexz_result, storage)

    print(f"\n{'=' * 60}")


if __name__ == "__main__":
    main()
