#!/usr/bin/env python3
"""
Example: LLM Fine-tuning with Per-Step Checkpoints — Mistral-7B

A fine-tuning workflow on Mistral-7B-v0.1 using the Alpaca instruction
dataset. Demonstrates how hexz.checkpoint creates a version history of
checkpoints with commit-log messages and chained XOR delta deduplication.

The checkpoint chain forms a history, much like git commits:

  step_0.hxz    ← pretrained baseline (~14 GB compressed)
  step_1.hxz    ← "lm_head fine-tune, step 1/5, loss=3.21"  (~10 MB)
  step_2.hxz    ← "lm_head fine-tune, step 2/5, loss=2.87"  (~10 MB)
  ...
  step_N.hxz    ← "deep fine-tune, step M/M, loss=1.42"     (~500 MB)

Because frozen layers are byte-for-byte identical across saves, hexz
stores only the changed weights. The `base=` parameter passes the
previous state dict in memory, eliminating page cache variance and
making save times consistent regardless of chain depth.

Device selection:
  If the GPU has enough VRAM to hold the model with headroom for training
  (roughly 1.5x model size), the model is loaded directly to GPU. Otherwise
  it is loaded to CPU — simpler code, no device_map hacks, standard
  load_state_dict works — at the cost of slower training. On a typical 8 GB
  consumer GPU, CPU mode is selected automatically. CPU step counts are set
  much lower so the demo finishes in reasonable time.

Requirements:
    pip install torch transformers datasets accelerate
"""

import gc
import os
import time

import hexz.checkpoint as ckpt
from hexz.utils import inspect as hexz_inspect
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

# Rough model size: 7B params x 2 bytes (float16) = 14 GB.
# Training needs ~1.5x that for activations + optimizer headroom.
_MODEL_BYTES = 7e9 * 2
_vram = (
    torch.cuda.get_device_properties(0).total_memory if torch.cuda.is_available() else 0
)
TRAIN_DEVICE = "cuda" if _vram >= _MODEL_BYTES * 1.5 else "cpu"

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------

MODEL_ID = "mistralai/Mistral-7B-v0.1"
_EXAMPLES_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CKPT_DIR = os.path.join(_EXAMPLES_DIR, ".data", "mistral")

BATCH_SIZE = 1
GRAD_ACCUM = 8
MAX_SAMPLES = 2_000

# CPU training is much slower; use fewer steps so the demo finishes.
if TRAIN_DEVICE == "cpu":
    MAX_SEQ_LEN = 64
    HEAD_STEPS = 5
    DEEP_STEPS = 10
    DEEP_SAVE_EVERY = 5
else:
    MAX_SEQ_LEN = 256
    HEAD_STEPS = 100
    DEEP_STEPS = 200
    DEEP_SAVE_EVERY = 50

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


def snapshot_state(model):
    """Clone model state_dict in storage dtype for checkpoint base.

    Converts any float32 tensors (from CPU upcast for optimizer stability)
    back to the storage dtype so frozen layers remain byte-identical for
    XOR delta deduplication.
    """
    sd = {}
    for k, v in model.state_dict().items():
        t = v.detach().clone()
        if t.is_floating_point() and t.dtype != TORCH_DTYPE:
            t = t.to(TORCH_DTYPE)
        sd[k] = t
    return sd


def train_with_checkpoints(
    model,
    loader,
    optimizer,
    n_steps,
    phase_name,
    parent_path,
    base_sd,
    scheduler=None,
    save_every=1,
):
    """Train with per-step checkpoint saves using chained XOR deltas.

    Each checkpoint is saved with:
    - parent= pointing to the previous checkpoint (for dedup)
    - base= with the cloned previous state dict (avoids loading parent from disk)
    - message= with a commit-log-style training summary

    Returns (avg_loss, final_path, final_sd, checkpoint_paths).
    """
    model.train()
    total_loss = 0.0
    microsteps = 0
    updates = 0

    prev_path = parent_path
    prev_sd = base_sd
    paths = []
    save_times = []

    data_iter = iter(loader)
    optimizer.zero_grad()
    t0 = time.time()
    t_step = t0

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
            avg_loss = total_loss / microsteps

            now = time.time()
            step_time = now - t_step
            t_step = now

            # Save checkpoint at interval or at the final step
            if updates % save_every == 0 or updates == n_steps:
                sd = snapshot_state(model)

                step_path = os.path.join(CKPT_DIR, f"{phase_name}_step{updates}.hxz")
                lr_str = ""
                if scheduler is not None:
                    lr_str = f", lr={optimizer.param_groups[0]['lr']:.2e}"
                message = (
                    f"{phase_name}, step {updates}/{n_steps}, "
                    f"loss={avg_loss:.4f}{lr_str}"
                )

                t_save = time.time()
                ckpt.save(
                    sd,
                    step_path,
                    parent=prev_path,
                    base=prev_sd,
                    message=message,
                )
                t_save = time.time() - t_save

                sz = os.path.getsize(step_path)
                save_times.append(t_save)
                paths.append(step_path)
                print(
                    f"  step {updates:>4}/{n_steps}"
                    f"  loss={avg_loss:.4f}"
                    f"  train={step_time:.1f}s"
                    f"  save={t_save:.2f}s"
                    f"  size={sz / 1e6:.1f}MB"
                    f'  "{message}"',
                    flush=True,
                )

                prev_path = step_path
                prev_sd = sd
            else:
                elapsed = now - t0
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
    if save_times:
        avg_save = sum(save_times) / len(save_times)
        print(f"  Avg save time: {avg_save:.2f}s over {len(save_times)} checkpoints")

    return total_loss / microsteps, prev_path, prev_sd, paths


def count_trainable(model):
    return sum(p.numel() for p in model.parameters() if p.requires_grad)


def count_frozen(model):
    return sum(p.numel() for p in model.parameters() if not p.requires_grad)


def section(title):
    print(f"\n{'=' * 65}")
    print(f"  {title}")
    print(f"{'=' * 65}")


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
    # no accelerate hooks. model.state_dict() returns real tensors and
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
    # Baseline: save pretrained weights (step 0)
    # -----------------------------------------------------------------------

    section("Baseline — Pretrained Mistral-7B (no training)")

    for param in model.parameters():
        param.requires_grad = False

    v0_path = os.path.join(CKPT_DIR, "step_0.hxz")
    if os.path.exists(v0_path):
        print(f"Skipping — checkpoint exists: {v0_path}")
    else:
        t0 = time.time()
        ckpt.save(
            model.state_dict(),
            v0_path,
            progress=True,
            message="pretrained mistral-7b baseline",
        )
        t_save0 = time.time() - t0
        sz0 = os.path.getsize(v0_path)
        print(f"Saved -> {v0_path}")
        print(f"  Size:      {sz0 / 1e9:.2f} GB")
        print(f"  Save time: {t_save0:.1f}s")

    # Snapshot baseline state for use as `base=` in first delta save
    base_sd = snapshot_state(model)
    all_paths = [v0_path]

    # -----------------------------------------------------------------------
    # Phase 1: Fine-tune lm_head only — save every step
    # -----------------------------------------------------------------------

    section(f"Phase 1 — lm_head fine-tune ({HEAD_STEPS} steps, save every step)")

    for name, param in model.named_parameters():
        param.requires_grad = name.startswith("lm_head.")

    print(
        f"Trainable params: {count_trainable(model):,}  "
        f"(frozen: {count_frozen(model):,})"
    )

    # Check if final checkpoint already exists
    head_final = os.path.join(CKPT_DIR, f"head_step{HEAD_STEPS}.hxz")
    if os.path.exists(head_final):
        print(f"Skipping — restoring from: {head_final}")
        model.load_state_dict(ckpt.load(head_final, device=TRAIN_DEVICE), strict=False)
        base_sd = snapshot_state(model)
        # Collect existing checkpoint paths for the history
        for s in range(1, HEAD_STEPS + 1):
            p = os.path.join(CKPT_DIR, f"head_step{s}.hxz")
            if os.path.exists(p):
                all_paths.append(p)
    else:
        # Upcast trainable params to float32 — bf16 optimizer math on CPU produces NaN.
        if TRAIN_DEVICE == "cpu":
            for p in model.parameters():
                if p.requires_grad:
                    p.data = p.data.float()

        optimizer = optim.AdamW(
            (p for p in model.parameters() if p.requires_grad), lr=2e-4
        )
        _, _, base_sd, head_paths = train_with_checkpoints(
            model,
            train_loader,
            optimizer,
            HEAD_STEPS,
            phase_name="head",
            parent_path=all_paths[-1],
            base_sd=base_sd,
            save_every=1,
        )
        all_paths.extend(head_paths)
        del optimizer
        gc.collect()

        # Upcast back so model is in training state for next phase
        if TRAIN_DEVICE == "cpu":
            for p in model.parameters():
                if p.requires_grad:
                    p.data = p.data.to(TORCH_DTYPE)

    # -----------------------------------------------------------------------
    # Phase 2: Unfreeze last 2 transformer blocks + lm_head — save periodically
    # -----------------------------------------------------------------------

    unfreeze_from = n_layers - 2

    section(
        f"Phase 2 — blocks {unfreeze_from}-{n_layers - 1} + lm_head "
        f"({DEEP_STEPS} steps, save every {DEEP_SAVE_EVERY})"
    )

    for name, param in model.named_parameters():
        in_last_blocks = any(
            name.startswith(f"model.layers.{i}.")
            for i in range(unfreeze_from, n_layers)
        )
        param.requires_grad = in_last_blocks or name.startswith("lm_head.")

    print(
        f"Trainable params: {count_trainable(model):,}  "
        f"(frozen: {count_frozen(model):,})"
    )

    deep_final = os.path.join(CKPT_DIR, f"deep_step{DEEP_STEPS}.hxz")
    if os.path.exists(deep_final):
        print(f"Skipping — checkpoint exists: {deep_final}")
        # Collect existing checkpoint paths
        for s in range(DEEP_SAVE_EVERY, DEEP_STEPS + 1, DEEP_SAVE_EVERY):
            p = os.path.join(CKPT_DIR, f"deep_step{s}.hxz")
            if os.path.exists(p):
                all_paths.append(p)
        if DEEP_STEPS % DEEP_SAVE_EVERY != 0:
            p = os.path.join(CKPT_DIR, f"deep_step{DEEP_STEPS}.hxz")
            if os.path.exists(p):
                all_paths.append(p)
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
        _, _, _, deep_paths = train_with_checkpoints(
            model,
            train_loader,
            optimizer,
            DEEP_STEPS,
            phase_name="deep",
            parent_path=all_paths[-1],
            base_sd=base_sd,
            scheduler=scheduler,
            save_every=DEEP_SAVE_EVERY,
        )
        all_paths.extend(deep_paths)
        del optimizer, scheduler
        gc.collect()

        if TRAIN_DEVICE == "cpu":
            for p in model.parameters():
                if p.requires_grad:
                    p.data = p.data.to(TORCH_DTYPE)

    # -----------------------------------------------------------------------
    # Checkpoint history — inspect the chain like git log
    # -----------------------------------------------------------------------

    section("Checkpoint history")

    total_chain_bytes = 0
    for p in all_paths:
        if os.path.exists(p):
            sz = os.path.getsize(p)
            total_chain_bytes += sz
            meta = ckpt.manifest(p) if p != all_paths[0] else None
            # Read the commit message from checkpoint metadata
            try:
                file_meta = hexz_inspect(p)
                msg = file_meta.get("message", "")
            except Exception:
                msg = ""

            if meta is not None:
                n_tensors = len(meta)
                xor_count = sum(
                    1 for v in meta.values() if v.get("storage") == "xor_delta"
                )
                print(
                    f"  {os.path.basename(p):<30}  {sz / 1e6:>8.1f} MB  "
                    f"({xor_count}/{n_tensors} xor_delta)"
                )
            else:
                print(f"  {os.path.basename(p):<30}  {sz / 1e9:>8.2f} GB  (baseline)")
            if msg:
                print(f"    {msg}")

    # -----------------------------------------------------------------------
    # Storage analysis
    # -----------------------------------------------------------------------

    section("Storage analysis")

    bytes_per_param = 2  # float16 / bfloat16
    full_mb = total_params * bytes_per_param / 1e6
    naive_mb = full_mb * len(all_paths)
    hexz_mb = total_chain_bytes / 1e6
    savings_pct = (1 - hexz_mb / naive_mb) * 100

    print(f"Model:              Mistral-7B-v0.1, {total_params / 1e9:.2f}B parameters")
    print(f"Checkpoint size:    {full_mb / 1e3:.2f} GB each (uncompressed)")
    print(f"Checkpoints saved:  {len(all_paths)}")
    print()
    print(
        f"  Naive ({len(all_paths)} x {full_mb / 1e3:.2f} GB):  {naive_mb / 1e3:>8.2f} GB"
    )
    print(f"  Hexz chain total:               {hexz_mb / 1e3:>8.2f} GB")
    print(f"  Savings:                         {savings_pct:>7.1f}%")

    # -----------------------------------------------------------------------
    # Latency: selective load vs full load
    # -----------------------------------------------------------------------

    section("Latency: selective load vs full load")

    final_path = all_paths[-1]
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
    partial = ckpt.load(final_path, keys=fine_tuned_keys)
    t_partial = time.time() - t0
    partial_mb = sum(v.nbytes for v in partial.values() if hasattr(v, "nbytes")) / 1e6

    t0 = time.time()
    full = ckpt.load(final_path)
    t_full = time.time() - t0
    full_mb = sum(v.nbytes for v in full.values() if hasattr(v, "nbytes")) / 1e6

    print(
        f"Full load:                {t_full:.3f}s  "
        f"({len(full):,} keys, {full_mb:.0f} MB)"
    )
    print(
        f"Fine-tuned layers only:   {t_partial * 1e3:.1f} ms  "
        f"({len(partial):,} keys, {partial_mb:.0f} MB)"
    )
    if t_partial > 0:
        print(f"Speedup:                  {t_full / t_partial:.0f}x")

    print()


if __name__ == "__main__":
    main()
