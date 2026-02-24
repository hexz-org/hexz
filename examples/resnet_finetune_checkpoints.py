#!/usr/bin/env python3
"""
Example: Transfer Learning Checkpoints with Hexz

A real fine-tuning workflow on CIFAR-10 using a ResNet-18 pretrained on
ImageNet. Demonstrates how hexz.checkpoint deduplications across three
checkpoints:

  v1  — pretrained ResNet-18 weights (baseline, nothing trained yet)
  v2  — fine-tune: classifier head only (all conv layers frozen)
  v3  — fine-tune: unfreeze layer4 block + classifier (deeper adaptation)

Because frozen layers are byte-for-byte identical across checkpoints, hexz
stores only the changed weights in v2 and v3. The rest is a reference back
to v1.

Requirements:
    pip install torch torchvision

Usage:
    python examples/resnet_finetune_checkpoints.py
"""

import os
import time

import torch
import torch.nn as nn
import torch.optim as optim
import torchvision
import torchvision.transforms as transforms
from torchvision.models import ResNet18_Weights, resnet18

import hexz.checkpoint as ckpt

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------

DEVICE = "cuda" if torch.cuda.is_available() else "cpu"
DATA_DIR = os.path.join(os.path.dirname(__file__), ".cifar10_data")
CKPT_DIR = os.path.join(os.path.dirname(__file__), ".ckpts")
BATCH_SIZE = 128
HEAD_EPOCHS = 3  # epochs training classifier head only
LAYER4_EPOCHS = 3  # epochs with layer4 + head unfrozen
NUM_WORKERS = 4

os.makedirs(CKPT_DIR, exist_ok=True)


# ---------------------------------------------------------------------------
# Data
# ---------------------------------------------------------------------------


def get_loaders():
    # ImageNet normalisation — matches the pretrained ResNet weights
    mean = (0.485, 0.456, 0.406)
    std = (0.229, 0.224, 0.225)

    train_tf = transforms.Compose(
        [
            transforms.RandomCrop(32, padding=4),
            transforms.RandomHorizontalFlip(),
            transforms.Resize(224),
            transforms.ToTensor(),
            transforms.Normalize(mean, std),
        ]
    )
    val_tf = transforms.Compose(
        [
            transforms.Resize(224),
            transforms.ToTensor(),
            transforms.Normalize(mean, std),
        ]
    )

    train_ds = torchvision.datasets.CIFAR10(
        DATA_DIR, train=True, download=True, transform=train_tf
    )
    val_ds = torchvision.datasets.CIFAR10(
        DATA_DIR, train=False, download=True, transform=val_tf
    )

    train_loader = torch.utils.data.DataLoader(
        train_ds,
        batch_size=BATCH_SIZE,
        shuffle=True,
        num_workers=NUM_WORKERS,
        pin_memory=True,
    )
    val_loader = torch.utils.data.DataLoader(
        val_ds,
        batch_size=256,
        shuffle=False,
        num_workers=NUM_WORKERS,
        pin_memory=True,
    )
    return train_loader, val_loader


# ---------------------------------------------------------------------------
# Training helpers
# ---------------------------------------------------------------------------


def train_epoch(model, loader, criterion, optimizer):
    model.train()
    total_loss, correct, n = 0.0, 0, 0
    for images, labels in loader:
        images, labels = images.to(DEVICE), labels.to(DEVICE)
        optimizer.zero_grad()
        out = model(images)
        loss = criterion(out, labels)
        loss.backward()
        optimizer.step()
        total_loss += loss.item() * len(labels)
        correct += out.argmax(1).eq(labels).sum().item()
        n += len(labels)
    return total_loss / n, correct / n


@torch.no_grad()
def evaluate(model, loader):
    model.eval()
    correct, n = 0, 0
    for images, labels in loader:
        images, labels = images.to(DEVICE), labels.to(DEVICE)
        correct += model(images).argmax(1).eq(labels).sum().item()
        n += len(labels)
    return correct / n


def count_trainable(model):
    return sum(p.numel() for p in model.parameters() if p.requires_grad)


def section(title):
    print(f"\n{'─' * 60}")
    print(f"  {title}")
    print(f"{'─' * 60}")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main():
    print(f"Device: {DEVICE}")
    if DEVICE == "cuda":
        print(f"GPU:    {torch.cuda.get_device_name(0)}")

    train_loader, val_loader = get_loaders()

    # -----------------------------------------------------------------------
    # v1: Save pretrained weights before any fine-tuning
    # -----------------------------------------------------------------------

    section("v1 — Save pretrained ResNet-18 (ImageNet weights)")

    model = resnet18(weights=ResNet18_Weights.IMAGENET1K_V1)
    # Replace classifier head for 10 CIFAR classes
    model.fc = nn.Linear(model.fc.in_features, 10)
    model = model.to(DEVICE)

    # Evaluate out-of-the-box accuracy (ImageNet weights on CIFAR-10)
    acc_pretrained = evaluate(model, val_loader)
    print(f"Pretrained accuracy on CIFAR-10 (no training): {acc_pretrained:.1%}")

    v1_path = os.path.join(CKPT_DIR, "v1_pretrained.hxz")
    state_v1 = {**model.state_dict(), "epoch": 0, "val_acc": round(acc_pretrained, 4)}
    ckpt.save(state_v1, v1_path)
    sz1 = os.path.getsize(v1_path)
    print(f"Saved → {v1_path}  ({sz1 / 1e6:.1f} MB)")

    # -----------------------------------------------------------------------
    # v2: Fine-tune classifier head only (all conv layers frozen)
    # -----------------------------------------------------------------------

    section(f"v2 — Fine-tune: classifier head only ({HEAD_EPOCHS} epochs)")

    # Freeze everything except the new fc layer
    for name, param in model.named_parameters():
        param.requires_grad = name.startswith("fc.")

    print(
        f"Trainable params: {count_trainable(model):,}  "
        f"(frozen: {sum(p.numel() for p in model.parameters() if not p.requires_grad):,})"
    )

    optimizer = optim.Adam(
        filter(lambda p: p.requires_grad, model.parameters()), lr=1e-3
    )
    criterion = nn.CrossEntropyLoss()

    for epoch in range(1, HEAD_EPOCHS + 1):
        t0 = time.time()
        loss, train_acc = train_epoch(model, train_loader, criterion, optimizer)
        val_acc = evaluate(model, val_loader)
        print(
            f"  Epoch {epoch}/{HEAD_EPOCHS}  "
            f"loss={loss:.4f}  train={train_acc:.1%}  val={val_acc:.1%}  "
            f"({time.time() - t0:.0f}s)"
        )

    v2_path = os.path.join(CKPT_DIR, "v2_head_only.hxz")
    state_v2 = {
        **model.state_dict(),
        "epoch": HEAD_EPOCHS,
        "val_acc": round(val_acc, 4),
    }
    ckpt.save(state_v2, v2_path, parent=v1_path)
    sz2 = os.path.getsize(v2_path)
    print(f"Saved → {v2_path}  ({sz2 / 1e6:.1f} MB)")

    # -----------------------------------------------------------------------
    # v3: Unfreeze layer4 + classifier, train deeper
    # -----------------------------------------------------------------------

    section(f"v3 — Fine-tune: layer4 + classifier ({LAYER4_EPOCHS} epochs)")

    # Unfreeze layer4 and fc
    for name, param in model.named_parameters():
        param.requires_grad = name.startswith("layer4.") or name.startswith("fc.")

    print(
        f"Trainable params: {count_trainable(model):,}  "
        f"(frozen: {sum(p.numel() for p in model.parameters() if not p.requires_grad):,})"
    )

    optimizer = optim.SGD(
        filter(lambda p: p.requires_grad, model.parameters()),
        lr=1e-3,
        momentum=0.9,
        weight_decay=1e-4,
    )
    scheduler = optim.lr_scheduler.CosineAnnealingLR(optimizer, T_max=LAYER4_EPOCHS)

    for epoch in range(1, LAYER4_EPOCHS + 1):
        t0 = time.time()
        loss, train_acc = train_epoch(model, train_loader, criterion, optimizer)
        val_acc = evaluate(model, val_loader)
        scheduler.step()
        print(
            f"  Epoch {epoch}/{LAYER4_EPOCHS}  "
            f"loss={loss:.4f}  train={train_acc:.1%}  val={val_acc:.1%}  "
            f"({time.time() - t0:.0f}s)"
        )

    v3_path = os.path.join(CKPT_DIR, "v3_layer4_head.hxz")
    state_v3 = {
        **model.state_dict(),
        "epoch": HEAD_EPOCHS + LAYER4_EPOCHS,
        "val_acc": round(val_acc, 4),
    }
    ckpt.save(state_v3, v3_path, parent=v2_path)
    sz3 = os.path.getsize(v3_path)
    print(f"Saved → {v3_path}  ({sz3 / 1e6:.1f} MB)")

    # -----------------------------------------------------------------------
    # Storage analysis
    # -----------------------------------------------------------------------

    section("Storage analysis")

    total_params = sum(p.numel() for p in model.parameters())
    uncompressed_mb = total_params * 4 / 1e6  # float32

    naive_mb = uncompressed_mb * 3
    hexz_mb = (sz1 + sz2 + sz3) / 1e6
    savings = (1 - hexz_mb / naive_mb) * 100

    print(f"Model:              ResNet-18, {total_params / 1e6:.1f}M parameters")
    print(f"Uncompressed size:  {uncompressed_mb:.1f} MB per checkpoint")
    print()
    print(f"  v1  pretrained (full)          {sz1 / 1e6:6.1f} MB")
    print(
        f"  v2  +head fine-tune            {sz2 / 1e6:6.1f} MB  ← only fc weights stored"
    )
    print(
        f"  v3  +layer4 fine-tune          {sz3 / 1e6:6.1f} MB  ← only layer4+fc stored"
    )
    print()
    print(f"  Naive (3 × {uncompressed_mb:.0f} MB):         {naive_mb:6.1f} MB")
    print(f"  Hexz chain total:              {hexz_mb:6.1f} MB")
    print(f"  Savings:                       {savings:.0f}%")

    # -----------------------------------------------------------------------
    # Selective load demo
    # -----------------------------------------------------------------------

    section("Selective load — restore just the classifier head")

    # Only load the fc weights to inspect or swap the head
    t0 = time.time()
    head_only = ckpt.load(v3_path, keys=["fc.weight", "fc.bias"])
    t_partial = time.time() - t0

    t0 = time.time()
    full_state = ckpt.load(v3_path)
    t_full = time.time() - t0

    print(f"Full model load:      {t_full * 1e3:.0f} ms  ({len(full_state)} keys)")
    print(f"Classifier head only: {t_partial * 1e3:.1f} ms  ({len(head_only)} keys)")
    print(f"Speedup:              {t_full / t_partial:.0f}×")

    # -----------------------------------------------------------------------
    # Manifest — inspect without loading any weights
    # -----------------------------------------------------------------------

    section("Manifest — inspect checkpoint structure (no data loaded)")

    info = ckpt.manifest(v3_path)
    total_bytes = sum(v["length"] for v in info.values())
    print(f"Checkpoint contains {len(info)} tensors ({total_bytes / 1e6:.1f} MB raw):")
    for name, meta in sorted(info.items())[:8]:
        print(f"  {name:<45}  {meta['dtype']}  {meta['shape']}")
    if len(info) > 8:
        print(f"  ... and {len(info) - 8} more")

    print()


if __name__ == "__main__":
    main()
