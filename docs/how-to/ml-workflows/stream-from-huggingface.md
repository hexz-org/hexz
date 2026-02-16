# Use Hosted Datasets

**Goal**: Download a pre-packed hexz dataset and train — one file, no extraction, no temp directories.

**Prerequisites**:
- Hexz Python package installed (`pip install hexz`)
- PyTorch installed (`pip install torch`)

## Problem

Setting up a dataset for training typically means downloading a raw archive, extracting it, and converting it into the right format. With hexz, you download a single `.hxz` file that's already compressed and ready to use.

## Solution

Hexz datasets are pre-packed with LZ4 compression and CDC deduplication. Download once, point `hexz.Dataset` at the file, and train.

## Step 1: Download a Hosted Dataset

Pre-packed datasets are available as [GitHub Release assets](https://github.com/hexz-org/hexz-examples/releases/tag/datasets-v1):

| Dataset | Files | Total size |
|---------|-------|------------|
| CIFAR-10 | `cifar10-train.hxz` + `cifar10-test.hxz` | ~173 MB |
| CIFAR-100 | `cifar100-train.hxz` + `cifar100-test.hxz` | ~170 MB |

```bash
# Download with curl
curl -LO https://github.com/hexz-org/hexz-examples/releases/download/datasets-v1/cifar100-train.hxz
curl -LO https://github.com/hexz-org/hexz-examples/releases/download/datasets-v1/cifar100-test.hxz

# Or with gh CLI
gh release download datasets-v1 --repo hexz-org/hexz-examples --pattern "cifar100-*"
```

## Step 2: Train

```python
import hexz
import torch

dataset = hexz.Dataset(
    "cifar100-train.hxz",
    item_size=3073,  # 1 byte label + 3072 bytes pixels (3x32x32)
    shuffle=True,
)

loader = torch.utils.data.DataLoader(dataset, batch_size=128)

for batch in loader:
    raw = batch.numpy()
    labels = raw[:, 0].astype("int64")
    pixels = raw[:, 1:].reshape(-1, 3, 32, 32).astype("float32") / 255.0
    # ... train your model
```

No `download=True`, no extraction, no temp directories.

## Step 3: Inspect Metadata

Each snapshot includes metadata describing its contents:

```python
import hexz

with hexz.open("cifar100-train.hxz") as reader:
    meta = reader.metadata()
    print(meta)
    # {'dataset': 'cifar100', 'split': 'train', 'item_size': 3073,
    #  'samples': 50000, 'num_classes': 100, 'image_shape': [3, 32, 32]}
```

## Host Your Own Datasets

Pack a dataset and upload it for others to use:

```python
import hexz

with hexz.Writer("train.hxz", compression="lz4", dedup=True, cdc=True) as w:
    w.add("train.raw")
    w.add_metadata({
        "dataset": "my-dataset",
        "split": "train",
        "item_size": 3073,
        "samples": 50000,
    })
```

Upload as a GitHub Release asset (free, up to 2 GB per file):
```bash
gh release create datasets-v1 train.hxz test.hxz --repo your-org/your-repo
```

Or upload to Hugging Face for discoverability:
```bash
pip install huggingface_hub
huggingface-cli login
huggingface-cli upload your-username/my-dataset train.hxz --repo-type dataset
```

See [hexz-examples/dataset-upload](https://github.com/hexz-org/hexz-examples/tree/main/dataset-upload) for a complete upload script with dataset card generation.

## Why Not Stream Directly?

Hexz supports block-level HTTP range requests for on-demand access (see [Setup S3 Streaming](setup-s3-streaming.md)). This is useful for inference or data exploration where you only need a subset. For training, downloading the file first is always faster — you'll read every sample multiple times across epochs, so the up-front download cost is negligible compared to repeated network fetches.

## See Also

- [Setup S3 Streaming](setup-s3-streaming.md) — on-demand access from S3 for inference and exploration
- [Streaming Best Practices](streaming-best-practices.md) — cache tuning, worker count, etc.
- [hexz-examples](https://github.com/hexz-org/hexz-examples) — complete training scripts
