#!/usr/bin/env python3
"""
Example: Per-Step Checkpoint Chain with Hexz

Trains ResNet-18 on CIFAR-10 and saves a checkpoint after every training
step, chained to the previous one. This demonstrates how hexz XOR delta +
byte-shuffle compresses consecutive-step weight changes:

  step_00.hxz  — pretrained (full snapshot)
  step_01.hxz  — after 1 gradient step  (parent: step_00)
  step_02.hxz  — after 2 gradient steps (parent: step_01)
  ...
  step_10.hxz  — after 10 gradient steps (parent: step_09)

Between consecutive steps, only a single gradient update changes the
trainable weights. The XOR delta has ~3 zero bytes per float, and
byte-shuffling groups those zeros into long runs that zstd compresses
dramatically.

Requirements:
    pip install torch torchvision

Usage:
    python examples/checkpoint/resnet_finetune_checkpoints.py
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
NUM_STEPS = 10
NUM_WORKERS = 4

os.makedirs(CKPT_DIR, exist_ok=True)


# ---------------------------------------------------------------------------
# Data
# ---------------------------------------------------------------------------


def get_loader():
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

    train_ds = torchvision.datasets.CIFAR10(
        DATA_DIR, train=True, download=True, transform=train_tf
    )

    train_loader = torch.utils.data.DataLoader(
        train_ds,
        batch_size=BATCH_SIZE,
        shuffle=True,
        num_workers=NUM_WORKERS,
        pin_memory=True,
    )
    return train_loader


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def count_trainable(model):
    return sum(p.numel() for p in model.parameters() if p.requires_grad)


def section(title):
    print(f"\n{'─' * 65}")
    print(f"  {title}")
    print(f"{'─' * 65}")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main():
    print(f"Device: {DEVICE}")
    if DEVICE == "cuda":
        print(f"GPU:    {torch.cuda.get_device_name(0)}")

    train_loader = get_loader()

    # -----------------------------------------------------------------------
    # Build model: ResNet-18, unfreeze layer4 + classifier head
    # -----------------------------------------------------------------------

    model = resnet18(weights=ResNet18_Weights.IMAGENET1K_V1)
    model.fc = nn.Linear(model.fc.in_features, 10)
    model = model.to(DEVICE)

    # Unfreeze layer4 + fc (the rest stays frozen)
    for name, param in model.named_parameters():
        param.requires_grad = name.startswith("layer4.") or name.startswith("fc.")

    total_params = sum(p.numel() for p in model.parameters())
    trainable = count_trainable(model)
    frozen = total_params - trainable
    uncompressed_mb = total_params * 4 / 1e6

    print(
        f"Model: ResNet-18  ({total_params / 1e6:.1f}M params, {uncompressed_mb:.1f} MB raw)"
    )
    print(f"Trainable: {trainable:,}  Frozen: {frozen:,}")

    optimizer = optim.SGD(
        filter(lambda p: p.requires_grad, model.parameters()),
        lr=1e-3,
        momentum=0.9,
        weight_decay=1e-4,
    )
    criterion = nn.CrossEntropyLoss()

    # -----------------------------------------------------------------------
    # Step 0: Save pretrained weights (full snapshot, no parent)
    # -----------------------------------------------------------------------

    section("step 0 — pretrained (full snapshot)")

    paths = []
    sizes = []

    step0_path = os.path.join(CKPT_DIR, "step_00.hxz")
    # Clone tensors: state_dict() returns views sharing storage with the model,
    # so optimizer.step() would mutate them in-place. We need frozen copies.
    prev_state = {
        k: v.clone() if isinstance(v, torch.Tensor) else v
        for k, v in model.state_dict().items()
    }
    prev_state["step"] = 0
    t0 = time.time()
    ckpt.save(prev_state, step0_path)
    save_time = time.time() - t0
    sz = os.path.getsize(step0_path)
    paths.append(step0_path)
    sizes.append(sz)
    print(f"Saved → {step0_path}  ({sz / 1e6:.2f} MB, {save_time:.2f}s)")

    # -----------------------------------------------------------------------
    # Steps 1..N: Train one batch, save checkpoint chained to previous
    # -----------------------------------------------------------------------

    section(f"Training {NUM_STEPS} steps, checkpoint after each")

    data_iter = iter(train_loader)
    model.train()

    for step in range(1, NUM_STEPS + 1):
        # Get next batch (wrap around if needed)
        try:
            images, labels = next(data_iter)
        except StopIteration:
            data_iter = iter(train_loader)
            images, labels = next(data_iter)

        images, labels = images.to(DEVICE), labels.to(DEVICE)

        # Single gradient step
        optimizer.zero_grad()
        loss = criterion(model(images), labels)
        loss.backward()
        optimizer.step()

        # Save checkpoint chained to previous.
        # Pass base=prev_state so save() doesn't need to recursively load
        # the entire parent chain — O(1) instead of O(chain_depth).
        step_path = os.path.join(CKPT_DIR, f"step_{step:02d}.hxz")
        parent_path = paths[-1]

        curr_state = {
            k: v.clone() if isinstance(v, torch.Tensor) else v
            for k, v in model.state_dict().items()
        }
        curr_state.update({"step": step, "loss": round(loss.item(), 4)})
        t0 = time.time()
        ckpt.save(
            curr_state,
            step_path,
            parent=parent_path,
            base=prev_state,
        )
        save_time = time.time() - t0
        prev_state = curr_state
        sz = os.path.getsize(step_path)
        paths.append(step_path)
        sizes.append(sz)

        print(
            f"  step {step:2d}/{NUM_STEPS}  "
            f"loss={loss.item():.4f}  "
            f"size={sz / 1e6:.2f} MB  "
            f"save={save_time:.2f}s"
        )

    # -----------------------------------------------------------------------
    # Storage analysis
    # -----------------------------------------------------------------------

    section("Storage analysis")

    print(f"{'Step':<8} {'Size':>10} {'vs raw':>10} {'vs prev':>10}")
    print(f"{'─' * 8} {'─' * 10} {'─' * 10} {'─' * 10}")

    for i, (path, sz) in enumerate(zip(paths, sizes)):
        name = f"step_{i:02d}"
        ratio = sz / (uncompressed_mb * 1e6) * 100
        if i == 0:
            vs_prev = "—"
        else:
            vs_prev = f"{sz / sizes[0] * 100:.1f}%"
        print(f"{name:<8} {sz / 1e6:>9.2f}M {ratio:>9.1f}% {vs_prev:>10}")

    total_hexz = sum(sizes)
    naive_total = uncompressed_mb * 1e6 * len(sizes)
    savings = (1 - total_hexz / naive_total) * 100

    print()
    print(
        f"Naive ({len(sizes)} × {uncompressed_mb:.0f} MB):  {naive_total / 1e6:.1f} MB"
    )
    print(f"Hexz chain total:          {total_hexz / 1e6:.1f} MB")
    print(f"Savings:                   {savings:.1f}%")

    # -----------------------------------------------------------------------
    # Verify: load last checkpoint, check correctness
    # -----------------------------------------------------------------------

    section("Verify: load last checkpoint")

    t0 = time.time()
    restored = ckpt.load(paths[-1])
    load_time = time.time() - t0

    current_state = model.state_dict()
    all_match = True
    for key in current_state:
        if isinstance(current_state[key], torch.Tensor):
            if not torch.equal(current_state[key].cpu(), restored[key].cpu()):
                print(f"  MISMATCH: {key}")
                all_match = False

    print(f"Load time: {load_time * 1e3:.0f} ms")
    print(f"All tensors match: {all_match}")
    print()


if __name__ == "__main__":
    main()
