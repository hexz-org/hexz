# Migrate from WebDataset to Hexz

**Goal**: Convert existing WebDataset tar shards to Hexz snapshots for better performance and simpler management.

## Prerequisites

- Existing WebDataset tar shards
- Hexz CLI and Python package installed
- Understanding of your current data layout

## Why Migrate

WebDataset limitations that Hexz solves:

| Issue | WebDataset | Hexz |
|-------|-----------|--------|
| File count | Thousands of tar shards | Single snapshot file |
| True shuffling | Limited to within shards | Global shuffling supported |
| Random access | Must read sequentially | O(log N) seeks |
| Updates | Rebalance all shards | Incremental with deduplication |
| Management | Track many files | Single file |

## Migration Strategies

### Strategy 1: Direct Conversion (Simple)

Convert all tar files into a single Hexz snapshot.

**Step 1: Extract tar shards to temporary directory**:
```bash
mkdir /tmp/webdataset_extracted
cd /path/to/webdataset/shards

for shard in *.tar; do
    tar -xf "$shard" -C /tmp/webdataset_extracted
done
```

**Step 2: Pack into Hexz snapshot**:
```bash
hexz data pack \
  --disk /tmp/webdataset_extracted \
  --output dataset.hxz \
  --compression zstd \
  --compression-level 9 \
  --cdc
```

**Step 3: Upload to S3 (if needed)**:
```bash
aws s3 cp dataset.hxz s3://my-bucket/datasets/
```

**Step 4: Update training code**:
```python
# Old WebDataset code
import webdataset as wds

dataset = wds.WebDataset("s3://bucket/shards/shard-{000000..000999}.tar")

# New Hexz code
import hexz

dataset = hexz.open("s3://bucket/datasets/dataset.hxz")
```

### Strategy 2: Preserve Shard Structure (Compatibility)

Keep separate snapshots per shard for gradual migration.

```bash
for shard in shard-*.tar; do
    # Extract to temp dir
    temp_dir=$(mktemp -d)
    tar -xf "$shard" -C "$temp_dir"

    # Convert to Hexz
    output="${shard%.tar}.hxz"
    hexz data pack \
      --disk "$temp_dir" \
      --output "$output" \
      --compression lz4

    # Cleanup
    rm -rf "$temp_dir"
done
```

Then update code to use multiple snapshots:
```python
import glob
import hexz

# Load all shard snapshots
shard_paths = glob.glob("shard-*.hxz")
readers = [hexz.open(path) for path in shard_paths]

# Custom Dataset to handle multiple snapshots
class MultiSnapshotDataset(torch.utils.data.Dataset):
    def __init__(self, readers, item_size):
        self.readers = readers
        self.item_size = item_size
        self.lengths = [r.size() // item_size for r in readers]
        self.cumulative = [0] + list(itertools.accumulate(self.lengths))

    def __len__(self):
        return sum(self.lengths)

    def __getitem__(self, idx):
        # Find which reader
        for i, (start, end) in enumerate(zip(self.cumulative[:-1], self.cumulative[1:])):
            if start <= idx < end:
                local_idx = idx - start
                offset = local_idx * self.item_size
                return self.readers[i].read(self.item_size, offset=offset)
```

### Strategy 3: Streaming Conversion (Large Datasets)

Convert without extracting all data to disk.

**Python script** (`convert_webdataset.py`):
```python
import tarfile
import hexz
import glob
from tqdm import tqdm

output_path = "dataset.hxz"
shard_pattern = "shard-*.tar"

with hexz.open(output_path, mode="w", compression="zstd", cdc=True) as writer:
    for shard_path in tqdm(sorted(glob.glob(shard_pattern))):
        with tarfile.open(shard_path, "r") as tar:
            for member in tar.getmembers():
                if member.isfile():
                    f = tar.extractfile(member)
                    data = f.read()
                    writer.write(data)

print(f"Conversion complete: {output_path}")
```

Run:
```bash
python convert_webdataset.py
```

## Code Migration Examples

### Before: WebDataset with Preprocessing

```python
import webdataset as wds
from torchvision import transforms

preproc = transforms.Compose([
    transforms.ToTensor(),
    transforms.Normalize(mean=[0.485, 0.456, 0.406],
                       std=[0.229, 0.224, 0.225])
])

dataset = (
    wds.WebDataset("s3://bucket/shards/shard-{000000..000999}.tar")
    .decode("pil")
    .to_tuple("jpg", "cls")
    .map_tuple(preproc, lambda x: x)
)

loader = torch.utils.data.DataLoader(
    dataset,
    batch_size=32,
    num_workers=4
)
```

### After: Hexz with Same Preprocessing

```python
import hexz
import torch
from torch.utils.data import Dataset, DataLoader
from torchvision import transforms
from PIL import Image
import io

preproc = transforms.Compose([
    transforms.ToTensor(),
    transforms.Normalize(mean=[0.485, 0.456, 0.406],
                       std=[0.229, 0.224, 0.225])
])

class ImageDataset(Dataset):
    def __init__(self, snapshot_path, transform=None):
        self.reader = hexz.open(snapshot_path)
        self.transform = transform
        # Build index (see tutorial for full implementation)
        self._build_index()

    def _build_index(self):
        # Index JPEG boundaries in snapshot
        # (See tutorials/first-ml-pipeline.md for complete example)
        pass

    def __len__(self):
        return len(self.images)

    def __getitem__(self, idx):
        jpeg_bytes = self.images[idx]
        image = Image.open(io.BytesIO(jpeg_bytes))
        label = self.labels[idx]  # From metadata

        if self.transform:
            image = self.transform(image)

        return image, label

dataset = ImageDataset(
    "s3://bucket/datasets/dataset.hxz",
    transform=preproc
)

loader = DataLoader(
    dataset,
    batch_size=32,
    shuffle=True,  # Now supports true shuffling!
    num_workers=4
)
```

## Performance Comparison

Benchmark on 1TB ImageNet dataset:

| Metric | WebDataset (1000 shards) | Hexz (single file) |
|--------|--------------------------|----------------------|
| Files to manage | 1000 | 1 |
| First epoch (S3) | 45 min | 38 min |
| Second epoch | 45 min | 12 min (cached) |
| Storage | 1.0 TB | 0.85 TB (with dedup) |
| Shuffling | Per-shard only | Global |
| Update 5% data | Rebalance all shards | Pack new version with dedup |

## Migration Checklist

- [ ] Backup existing WebDataset shards
- [ ] Test conversion on small subset (1-2 shards)
- [ ] Verify data integrity after conversion
- [ ] Update training code
- [ ] Run training on small number of epochs to validate
- [ ] Convert full dataset
- [ ] Upload to S3/final storage
- [ ] Update production training scripts
- [ ] Monitor performance in production
- [ ] Archive old WebDataset shards

## Rollback Plan

Keep WebDataset shards until confident in Hexz migration:

1. Test Hexz for 2-3 full training runs
2. Compare validation metrics with WebDataset baseline
3. Only delete WebDataset shards after validation

## Common Issues

**"Out of memory during conversion"**:
- Use streaming conversion (Strategy 3)
- Process shards in batches

**"Hexz snapshot larger than expected"**:
- Ensure `--cdc` flag is enabled for deduplication
- Check if data is already compressed (JPEGs won't compress further)

**"Slower than WebDataset"**:
- Increase DataLoader `num_workers`
- Increase cache size
- See [Optimize PyTorch DataLoader](optimize-pytorch-dataloader.md)

## See Also

- [Tutorial: First ML Pipeline](../../tutorials/first-ml-pipeline.md)
- [How-To: Optimize PyTorch DataLoader](optimize-pytorch-dataloader.md)
- [How-To: Setup S3 Streaming](setup-s3-streaming.md)
