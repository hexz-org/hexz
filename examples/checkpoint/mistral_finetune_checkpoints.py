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

Device selection:
  If the GPU has enough VRAM to hold the model with headroom for training
  (roughly 1.5× model size), the model is loaded directly to GPU.  Otherwise
  it is loaded to CPU — simpler code, no device_map hacks, standard
  load_state_dict works — at the cost of slower training.  On a typical 8 GB
  consumer GPU, CPU mode is selected automatically.  CPU step counts are set
  much lower so the demo finishes in reasonable time.

Requirements:
    pip install torch transformers datasets accelerate
"""

import gc
import os
import time

import hexz.checkpoint as ckpt
import torch
import torch.optim as optim
from datasets import load_dataset
from torch.utils.data import DataLoader
from transformers import AutoModelForCausalLM, AutoTokenizer

# ---------------------------------------------------------------------------
# Device selection
# ---------------------------------------------------------------------------

# float16 overflows to NaN on CPU (no native fp16 math, tiny dynamic range).
# bfloat16 has float32's exponent range and is numerically stable on CPU.
# On CUDA, float16 is fine and slightly faster; use it if available.
TORCH_DTYPE = torch.float16 if torch.cuda.is_available() else torch.bfloat16

# Rough model size: 7B params × 2 bytes (float16) ≈ 14 GB.
# Training needs ~1.5× that for activations + optimizer headroom.
_MODEL_BYTES = 7e9 * 2
_vram = (
    torch.cuda.get_device_properties(0).total_memory if torch.cuda.is_available() else 0
)
TRAIN_DEVICE = "cuda" if _vram >= _MODEL_BYTES * 1.5 else "cpu"

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------

MODEL_ID = "mistralai/Mistral-7B-v0.1"
CKPT_DIR = os.path.join(os.path.dirname(__file__), ".ckpts")

BATCH_SIZE = 1
GRAD_ACCUM = 8
MAX_SAMPLES = 2_000

# CPU training is much slower; use fewer steps so the demo finishes.
if TRAIN_DEVICE == "cpu":
    MAX_SEQ_LEN = 64
    HEAD_STEPS = 5
    DEEP_STEPS = 10
else:
    MAX_SEQ_LEN = 256
    HEAD_STEPS = 100
    DEEP_STEPS = 200

os.makedirs(CKPT_DIR, exist_ok=True)
os.environ.setdefault("PYTORCH_ALLOC_CONF", "expandable_segments:True")


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
        pin_memory=(TRAIN_DEVICE == "cuda"),
    )


# ---------------------------------------------------------------------------
# Training helpers
# ---------------------------------------------------------------------------


def train_steps(model, loader, optimizer, n_steps, scheduler=None):
    """Run exactly n_steps gradient updates.  Returns (avg_loss, elapsed, secs/step)."""
    model.train()
    total_loss = 0.0
    microsteps = 0
    updates = 0
    t0 = time.time()
    t_step = t0

    data_iter = iter(loader)
    optimizer.zero_grad()

    while updates < n_steps:
        try:
            batch = next(data_iter)
        except StopIteration:
            data_iter = iter(loader)
            batch = next(data_iter)

        input_ids = batch["input_ids"].to(TRAIN_DEVICE)
        attention_mask = batch["attention_mask"].to(TRAIN_DEVICE)
        labels = batch["labels"].to(TRAIN_DEVICE)

        with torch.autocast(device_type=TRAIN_DEVICE, dtype=TORCH_DTYPE):
            out = model(
                input_ids=input_ids, attention_mask=attention_mask, labels=labels
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

            now = time.time()
            step_time = now - t_step
            t_step = now
            elapsed = now - t0
            avg_loss = total_loss / microsteps
            remaining = (n_steps - updates) * step_time
            print(
                f"  step {updates:>4}/{n_steps}"
                f"  loss={avg_loss:.4f}"
                f"  step={step_time:.1f}s"
                f"  elapsed={elapsed:.0f}s"
                f"  eta={remaining:.0f}s",
                flush=True,
            )

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
    print(f"Training device: {TRAIN_DEVICE}")
    if TRAIN_DEVICE == "cuda":
        props = torch.cuda.get_device_properties(0)
        print(f"GPU:    {props.name}")
        print(f"VRAM:   {props.total_memory / 1e9:.1f} GB")
    else:
        print("(CPU mode: fewer steps, but no device_map complexity)")

    # -----------------------------------------------------------------------
    # Load model & tokenizer
    # -----------------------------------------------------------------------

    section("Loading Mistral-7B-v0.1")

    t0 = time.time()
    tokenizer = AutoTokenizer.from_pretrained(MODEL_ID)
    tokenizer.pad_token = tokenizer.eos_token

    # device_map=TRAIN_DEVICE loads everything to one device — no meta tensors,
    # no accelerate hooks.  model.state_dict() returns real tensors and
    # model.load_state_dict() works normally.
    model = AutoModelForCausalLM.from_pretrained(
        MODEL_ID,
        dtype=TORCH_DTYPE,
        device_map=TRAIN_DEVICE,
    )
    t_load = time.time() - t0

    total_params = sum(p.numel() for p in model.parameters())
    print(f"Loaded in {t_load:.1f}s  |  {total_params / 1e9:.2f}B parameters")

    n_layers = len(model.model.layers)
    print(f"Transformer blocks: {n_layers}")

    train_loader = get_dataloader(tokenizer)

    # -----------------------------------------------------------------------
    # v1: Save pretrained weights before any fine-tuning
    # -----------------------------------------------------------------------

    section("v1 — Pretrained Mistral-7B (no training)")

    for param in model.parameters():
        param.requires_grad = False

    v1_path = os.path.join(CKPT_DIR, "v1_pretrained.hxz")
    if os.path.exists(v1_path):
        print(f"Skipping — checkpoint exists: {v1_path}")
        sz1 = os.path.getsize(v1_path)
    else:
        t0 = time.time()
        ckpt.save(model.state_dict(), v1_path, progress=True)
        t_save1 = time.time() - t0
        sz1 = os.path.getsize(v1_path)
        print(f"Saved → {v1_path}")
        print(f"  Size:      {sz1 / 1e9:.2f} GB")
        print(f"  Save time: {t_save1:.1f}s")

    # -----------------------------------------------------------------------
    # v2: Fine-tune lm_head only
    # -----------------------------------------------------------------------

    section(f"v2 — Fine-tune: lm_head only ({HEAD_STEPS} steps)")

    for name, param in model.named_parameters():
        param.requires_grad = name.startswith("lm_head.")

    print(
        f"Trainable params: {count_trainable(model):,}  (frozen: {count_frozen(model):,})"
    )

    v2_path = os.path.join(CKPT_DIR, "v2_head_only.hxz")
    if os.path.exists(v2_path):
        print(f"Skipping — restoring weights from: {v2_path}")
        model.load_state_dict(ckpt.load(v2_path, device=TRAIN_DEVICE), strict=False)
        sz2 = os.path.getsize(v2_path)
    else:
        # Upcast trainable params to float32 — bf16 optimizer math on CPU produces NaN.
        if TRAIN_DEVICE == "cpu":
            for p in model.parameters():
                if p.requires_grad:
                    p.data = p.data.float()

        optimizer = optim.AdamW(
            (p for p in model.parameters() if p.requires_grad), lr=2e-4
        )
        loss2, t_train2, sps2 = train_steps(model, train_loader, optimizer, HEAD_STEPS)
        print(
            f"Training done: loss={loss2:.4f}  total={t_train2:.0f}s  per-step={sps2:.1f}s"
        )
        del optimizer
        gc.collect()

        # Downcast back to storage dtype so frozen layers stay byte-identical for dedup.
        if TRAIN_DEVICE == "cpu":
            for p in model.parameters():
                if p.requires_grad:
                    p.data = p.data.to(TORCH_DTYPE)

        t0 = time.time()
        ckpt.save(model.state_dict(), v2_path, parent=v1_path, progress=True)
        t_save2 = time.time() - t0
        sz2 = os.path.getsize(v2_path)
        print(f"Saved → {v2_path}")
        print(f"  Size:      {sz2 / 1e6:.1f} MB  ← only lm_head weights stored")
        print(f"  Save time: {t_save2:.3f}s")

    # -----------------------------------------------------------------------
    # v3: Unfreeze last 2 transformer blocks + lm_head
    # -----------------------------------------------------------------------

    unfreeze_from = n_layers - 2

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

    v3_path = os.path.join(CKPT_DIR, "v3_last2blocks_head.hxz")
    if os.path.exists(v3_path):
        print(f"Skipping — checkpoint exists: {v3_path}")
        sz3 = os.path.getsize(v3_path)
    else:
        gc.collect()
        if TRAIN_DEVICE == "cuda":
            torch.cuda.empty_cache()

        if TRAIN_DEVICE == "cpu":
            for p in model.parameters():
                if p.requires_grad:
                    p.data = p.data.float()

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
        del optimizer, scheduler
        gc.collect()

        if TRAIN_DEVICE == "cpu":
            for p in model.parameters():
                if p.requires_grad:
                    p.data = p.data.to(TORCH_DTYPE)

        t0 = time.time()
        ckpt.save(model.state_dict(), v3_path, parent=v2_path, progress=True)
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
    partial_mb = sum(v.nbytes for v in partial.values() if hasattr(v, "nbytes")) / 1e6

    t0 = time.time()
    full = ckpt.load(v3_path)
    t_full = time.time() - t0
    full_delta_mb = sum(v.nbytes for v in full.values() if hasattr(v, "nbytes")) / 1e6

    print(
        f"Full v3 load (delta only):             {t_full:.3f}s  "
        f"({len(full):,} keys, {full_delta_mb:.0f} MB)"
    )
    print(
        f"Fine-tuned layers only:                {t_partial * 1e3:.1f} ms  "
        f"({len(partial):,} keys, {partial_mb:.0f} MB)"
    )
    if t_partial > 0:
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
