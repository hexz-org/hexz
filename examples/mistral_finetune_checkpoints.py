#!/usr/bin/env python3
"""
Example: LLM Checkpoint Deduplication with Hexz — Mistral-7B

A fine-tuning workflow on Mistral-7B-v0.1 using the Alpaca instruction
dataset. Demonstrates how hexz.checkpoint deduplicates across three
checkpoints:

  v1  — pretrained Mistral-7B weights (baseline, nothing trained yet)
  v2  — fine-tune: lm_head only (all 32 transformer blocks frozen)
  v3  — fine-tune: last 2 transformer blocks + lm_head (deeper adaptation)

Because frozen layers are byte-for-byte identical across checkpoints, hexz
stores only the changed weights in v2 and v3. On a 7B model the savings are
dramatic: v2 and v3 together add just ~tens of MB on top of the ~14 GB
baseline.

Hardware requirements (float16):
  ~14 GB VRAM for a single GPU. If you are short on memory, set
  TORCH_DTYPE = torch.bfloat16 and ensure you have a GPU that supports it,
  or reduce MAX_SEQ_LEN / BATCH_SIZE.

Requirements:
    pip install torch transformers datasets accelerate

Usage:
    python examples/mistral_finetune_checkpoints.py
"""

import os
import time

import hexz.checkpoint as ckpt
import torch
import torch.optim as optim
from datasets import load_dataset
from torch.utils.data import DataLoader
from transformers import AutoModelForCausalLM, AutoTokenizer

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------

MODEL_ID = "mistralai/Mistral-7B-v0.1"
DEVICE = "cpu"  # "cuda" if torch.cuda.is_available() else "cpu"
TORCH_DTYPE = torch.float16
CKPT_DIR = os.path.join(os.path.dirname(__file__), ".ckpts")

# Training hyper-params — kept small so the demo finishes quickly.
# Increase HEAD_STEPS / DEEP_STEPS for meaningful accuracy improvement.
BATCH_SIZE = 2
GRAD_ACCUM = 8  # effective batch size = 16
MAX_SEQ_LEN = 512
MAX_SAMPLES = 2_000  # subset of Alpaca to use
HEAD_STEPS = 100  # gradient updates training lm_head only
DEEP_STEPS = 200  # gradient updates with last-2 blocks + lm_head

os.makedirs(CKPT_DIR, exist_ok=True)


# ---------------------------------------------------------------------------
# Data
# ---------------------------------------------------------------------------


def get_dataloader(tokenizer):
    """Small slice of the Alpaca instruction-following dataset."""
    ds = load_dataset("tatsu-lab/alpaca", split=f"train[:{MAX_SAMPLES}]")

    def tokenize(example):
        text = example["instruction"]
        if example.get("input"):
            text += "\n\n" + example["input"]
        text += "\n\n### Response:\n" + example["output"]
        enc = tokenizer(
            text,
            max_length=MAX_SEQ_LEN,
            truncation=True,
            padding="max_length",
            return_tensors="pt",
        )
        enc["labels"] = enc["input_ids"].clone()
        return {k: v.squeeze(0) for k, v in enc.items()}

    ds = ds.map(tokenize, remove_columns=ds.column_names)
    ds.set_format(type="torch")
    return DataLoader(
        ds,
        batch_size=BATCH_SIZE,
        shuffle=True,
        pin_memory=(DEVICE == "cuda"),
    )


# ---------------------------------------------------------------------------
# Training helpers
# ---------------------------------------------------------------------------


def train_steps(model, loader, optimizer, n_steps, scheduler=None):
    """
    Run exactly n_steps gradient updates.

    Returns (avg_loss, elapsed_seconds, seconds_per_step).
    """
    model.train()
    total_loss = 0.0
    microsteps = 0
    updates = 0
    t0 = time.time()

    data_iter = iter(loader)
    optimizer.zero_grad()

    while updates < n_steps:
        try:
            batch = next(data_iter)
        except StopIteration:
            data_iter = iter(loader)
            batch = next(data_iter)

        input_ids = batch["input_ids"].to(DEVICE)
        attention_mask = batch["attention_mask"].to(DEVICE)
        labels = batch["labels"].to(DEVICE)

        out = model(
            input_ids=input_ids,
            attention_mask=attention_mask,
            labels=labels,
        )
        loss = out.loss / GRAD_ACCUM
        loss.backward()
        total_loss += out.loss.item()
        microsteps += 1

        if microsteps % GRAD_ACCUM == 0:
            torch.nn.utils.clip_grad_norm_(
                (p for p in model.parameters() if p.requires_grad), 1.0
            )
            optimizer.step()
            if scheduler is not None:
                scheduler.step()
            optimizer.zero_grad()
            updates += 1

    elapsed = time.time() - t0
    return total_loss / microsteps, elapsed, elapsed / n_steps


def count_trainable(model):
    return sum(p.numel() for p in model.parameters() if p.requires_grad)


def count_frozen(model):
    return sum(p.numel() for p in model.parameters() if not p.requires_grad)


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
        props = torch.cuda.get_device_properties(0)
        print(f"GPU:    {props.name}")
        print(f"VRAM:   {props.total_memory / 1e9:.1f} GB")

    # -----------------------------------------------------------------------
    # Load model & tokenizer
    # -----------------------------------------------------------------------

    section("Loading Mistral-7B-v0.1")

    t0 = time.time()
    tokenizer = AutoTokenizer.from_pretrained(MODEL_ID)
    tokenizer.pad_token = tokenizer.eos_token  # Mistral has no pad token

    model = AutoModelForCausalLM.from_pretrained(
        MODEL_ID,
        torch_dtype=TORCH_DTYPE,
    ).to(DEVICE)
    t_load = time.time() - t0

    total_params = sum(p.numel() for p in model.parameters())
    print(f"Loaded in {t_load:.1f}s  |  {total_params / 1e9:.2f}B parameters")

    n_layers = len(model.model.layers)  # 32 for Mistral-7B
    print(f"Transformer blocks: {n_layers}")

    train_loader = get_dataloader(tokenizer)

    # -----------------------------------------------------------------------
    # v1: Save pretrained weights before any fine-tuning
    # -----------------------------------------------------------------------

    section("v1 — Pretrained Mistral-7B (no training)")

    for param in model.parameters():
        param.requires_grad = False

    v1_path = os.path.join(CKPT_DIR, "v1_pretrained.hxz")
    t0 = time.time()
    ckpt.save(model.state_dict(), v1_path)
    t_save1 = time.time() - t0
    sz1 = os.path.getsize(v1_path)

    print(f"Saved → {v1_path}")
    print(f"  Size:      {sz1 / 1e9:.2f} GB")
    print(f"  Save time: {t_save1:.1f}s")

    # -----------------------------------------------------------------------
    # v2: Fine-tune lm_head only (all transformer blocks frozen)
    # -----------------------------------------------------------------------

    section(f"v2 — Fine-tune: lm_head only ({HEAD_STEPS} steps)")

    for name, param in model.named_parameters():
        param.requires_grad = name.startswith("lm_head.")

    lm_head_params = count_trainable(model)
    print(f"Trainable params: {lm_head_params:,}  (frozen: {count_frozen(model):,})")

    optimizer = optim.AdamW(
        (p for p in model.parameters() if p.requires_grad),
        lr=2e-4,
    )

    loss2, t_train2, sps2 = train_steps(model, train_loader, optimizer, HEAD_STEPS)
    print(
        f"Training done: loss={loss2:.4f}  total={t_train2:.0f}s  per-step={sps2:.1f}s"
    )

    v2_path = os.path.join(CKPT_DIR, "v2_head_only.hxz")
    t0 = time.time()
    ckpt.save(model.state_dict(), v2_path, parent=v1_path)
    t_save2 = time.time() - t0
    sz2 = os.path.getsize(v2_path)

    print(f"Saved → {v2_path}")
    print(f"  Size:      {sz2 / 1e6:.1f} MB  ← only lm_head weights stored")
    print(f"  Save time: {t_save2:.3f}s")

    # -----------------------------------------------------------------------
    # v3: Unfreeze last 2 transformer blocks + lm_head
    # -----------------------------------------------------------------------

    unfreeze_from = n_layers - 2  # blocks 30 & 31 for Mistral-7B

    section(
        f"v3 — Fine-tune: blocks {unfreeze_from}–{n_layers - 1} + lm_head "
        f"({DEEP_STEPS} steps)"
    )

    for name, param in model.named_parameters():
        in_last_blocks = any(
            name.startswith(f"model.layers.{i}.")
            for i in range(unfreeze_from, n_layers)
        )
        param.requires_grad = in_last_blocks or name.startswith("lm_head.")

    print(
        f"Trainable params: {count_trainable(model):,}  (frozen: {count_frozen(model):,})"
    )

    optimizer = optim.AdamW(
        (p for p in model.parameters() if p.requires_grad),
        lr=5e-5,
        weight_decay=0.01,
    )
    scheduler = optim.lr_scheduler.CosineAnnealingLR(optimizer, T_max=DEEP_STEPS)

    loss3, t_train3, sps3 = train_steps(
        model, train_loader, optimizer, DEEP_STEPS, scheduler=scheduler
    )
    print(
        f"Training done: loss={loss3:.4f}  total={t_train3:.0f}s  per-step={sps3:.1f}s"
    )

    v3_path = os.path.join(CKPT_DIR, "v3_last2blocks_head.hxz")
    t0 = time.time()
    ckpt.save(model.state_dict(), v3_path, parent=v2_path)
    t_save3 = time.time() - t0
    sz3 = os.path.getsize(v3_path)

    print(f"Saved → {v3_path}")
    print(
        f"  Size:      {sz3 / 1e6:.1f} MB  "
        f"← only blocks {unfreeze_from}–{n_layers - 1} + lm_head stored"
    )
    print(f"  Save time: {t_save3:.3f}s")

    # -----------------------------------------------------------------------
    # Storage analysis
    # -----------------------------------------------------------------------

    section("Storage analysis")

    bytes_per_param = 2  # float16
    full_mb = total_params * bytes_per_param / 1e6

    naive_mb = full_mb * 3
    hexz_mb = (sz1 + sz2 + sz3) / 1e6
    savings_pct = (1 - hexz_mb / naive_mb) * 100

    print(f"Model:              Mistral-7B-v0.1, {total_params / 1e9:.2f}B parameters")
    print(f"Checkpoint size:    {full_mb / 1e3:.2f} GB each (float16, uncompressed)")
    print()
    print(f"  v1  pretrained (full)                 {sz1 / 1e9:7.2f} GB")
    print(
        f"  v2  +lm_head fine-tune                {sz2 / 1e6:7.1f} MB  ← lm_head only"
    )
    print(
        f"  v3  +last-2-blocks fine-tune          {sz3 / 1e6:7.1f} MB  "
        f"← blocks {unfreeze_from}-{n_layers - 1} + lm_head"
    )
    print()
    print(f"  Naive (3 × {full_mb / 1e3:.2f} GB):          {naive_mb / 1e3:7.2f} GB")
    print(f"  Hexz chain total:                     {hexz_mb / 1e3:7.2f} GB")
    print(f"  Savings:                              {savings_pct:.1f}%")

    # -----------------------------------------------------------------------
    # Latency: selective load vs full load
    # -----------------------------------------------------------------------

    section("Latency: selective load vs full load from v3")

    # Keys present only in v3 (the delta): last block + lm_head
    state_keys = list(model.state_dict().keys())
    fine_tuned_keys = [
        k
        for k in state_keys
        if any(
            k.startswith(f"model.layers.{i}.") for i in range(unfreeze_from, n_layers)
        )
        or k.startswith("lm_head.")
    ]

    t0 = time.time()
    partial = ckpt.load(v3_path, keys=fine_tuned_keys)
    t_partial = time.time() - t0
    partial_mb = sum(v.nbytes for v in partial.values()) / 1e6

    t0 = time.time()
    full = ckpt.load(v3_path)
    t_full = time.time() - t0
    full_delta_mb = sum(v.nbytes for v in full.values()) / 1e6

    print(
        f"Full v3 load (delta + chain resolve):  {t_full:.3f}s  "
        f"({len(full):,} keys, {full_delta_mb:.0f} MB)"
    )
    print(
        f"Fine-tuned layers only:                {t_partial * 1e3:.1f} ms  "
        f"({len(partial):,} keys, {partial_mb:.0f} MB)"
    )
    print(f"Speedup:                               {t_full / t_partial:.0f}×")

    # -----------------------------------------------------------------------
    # Manifest — inspect without loading any weights
    # -----------------------------------------------------------------------

    section("Manifest — inspect v3 delta (no weights loaded)")

    info = ckpt.manifest(v3_path)
    delta_bytes = sum(v["length"] for v in info.values())
    print(f"v3 stores {len(info)} tensors ({delta_bytes / 1e6:.1f} MB raw):")
    for name, meta in sorted(info.items())[:10]:
        print(f"  {name:<60}  {meta['dtype']}  {meta['shape']}")
    if len(info) > 10:
        print(f"  ... and {len(info) - 10} more")

    print()


if __name__ == "__main__":
    main()
