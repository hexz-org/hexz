# Strata Python Loader — Examples

This guide provides complete, runnable examples for common ML/AI use cases.

## Example 1: ImageNet Training with PyTorch

Complete dataset wrapper for ImageNet stored in Strata format.

```python
"""
ImageNet dataset using Strata snapshots.

Snapshot format:
  [8 bytes: num_images (little-endian uint64)]
  [num_images * 16 bytes: index entries]
    Each entry: [8 bytes offset][8 bytes length]
  [Image data: JPEG files back-to-back]
"""

import torch
from torch.utils.data import Dataset, DataLoader
import torchvision.transforms as transforms
import strata
import numpy as np
from PIL import Image
import io

class ImageNetStrata(Dataset):
    def __init__(self, snapshot_path, split='train', transform=None):
        """
        Args:
            snapshot_path: Path to .st file (local or s3://)
            split: 'train' or 'val' (for label lookup)
            transform: torchvision transforms
        """
        self.reader = strata.open(snapshot_path)
        self.transform = transform or self._default_transform(split)

        # Read metadata header
        header = self.reader.read(8, offset=0)
        self.num_images = int.from_bytes(header, 'little')

        # Load index into memory (16 bytes per image)
        index_size = self.num_images * 16
        index_bytes = self.reader.read(index_size, offset=8)
        self.index = np.frombuffer(index_bytes, dtype=np.uint64).reshape(-1, 2)

        # Offset to account for header + index
        self.data_offset = 8 + index_size

    def _default_transform(self, split):
        if split == 'train':
            return transforms.Compose([
                transforms.RandomResizedCrop(224),
                transforms.RandomHorizontalFlip(),
                transforms.ToTensor(),
                transforms.Normalize(
                    mean=[0.485, 0.456, 0.406],
                    std=[0.229, 0.224, 0.225]
                )
            ])
        else:
            return transforms.Compose([
                transforms.Resize(256),
                transforms.CenterCrop(224),
                transforms.ToTensor(),
                transforms.Normalize(
                    mean=[0.485, 0.456, 0.406],
                    std=[0.229, 0.224, 0.225]
                )
            ])

    def __len__(self):
        return self.num_images

    def __getitem__(self, idx):
        # Get offset and length for this image
        offset, length = self.index[idx]
        actual_offset = self.data_offset + offset

        # Read JPEG bytes
        jpeg_bytes = self.reader.read(int(length), offset=actual_offset)

        # Decode to PIL Image
        image = Image.open(io.BytesIO(jpeg_bytes)).convert('RGB')

        # Label is encoded in filename or separate index
        # For this example, assume labels are in separate file
        label = self._get_label(idx)

        if self.transform:
            image = self.transform(image)

        return image, label

    def _get_label(self, idx):
        # In practice, load from labels.npy or embed in snapshot
        # This is a placeholder
        return idx % 1000  # 1000 ImageNet classes


def train_imagenet():
    """Training loop example."""
    # Load dataset
    train_dataset = ImageNetStrata(
        snapshot_path="s3://my-bucket/imagenet-train.st",
        split='train'
    )

    # Create DataLoader with multiple workers for parallel loading
    train_loader = DataLoader(
        train_dataset,
        batch_size=256,
        shuffle=True,
        num_workers=8,      # Parallel decompression
        pin_memory=True,    # Fast GPU transfer
        persistent_workers=True  # Reuse workers
    )

    # Your model
    model = torch.hub.load('pytorch/vision', 'resnet50', pretrained=False)
    model = model.cuda()

    optimizer = torch.optim.SGD(model.parameters(), lr=0.1, momentum=0.9)
    criterion = torch.nn.CrossEntropyLoss()

    # Training loop
    model.train()
    for epoch in range(90):
        for batch_idx, (images, labels) in enumerate(train_loader):
            images = images.cuda()
            labels = labels.cuda()

            optimizer.zero_grad()
            outputs = model(images)
            loss = criterion(outputs, labels)
            loss.backward()
            optimizer.step()

            if batch_idx % 100 == 0:
                print(f"Epoch {epoch}, Batch {batch_idx}, Loss: {loss.item():.4f}")


if __name__ == '__main__':
    train_imagenet()
```

## Example 2: Medical Imaging — NIfTI Volumes

Working with large 3D medical imaging datasets.

```python
"""
Brain MRI dataset from Strata snapshots.

Each volume is stored as:
  [4 bytes: width][4 bytes: height][4 bytes: depth]
  [width * height * depth * 4 bytes: float32 voxel data]
"""

import strata
import numpy as np
import torch
from torch.utils.data import Dataset

class BrainMRIStrata(Dataset):
    def __init__(self, snapshot_path, transform=None):
        self.reader = strata.open(snapshot_path)
        self.transform = transform

        # Read number of volumes
        num_vols_bytes = self.reader.read(8, offset=0)
        self.num_volumes = int.from_bytes(num_vols_bytes, 'little')

        # Build index of (offset, dimensions) for each volume
        self.volumes = []
        current_offset = 8

        for _ in range(self.num_volumes):
            # Read dimensions
            dim_bytes = self.reader.read(12, offset=current_offset)
            dims = np.frombuffer(dim_bytes, dtype=np.uint32)
            w, h, d = dims

            data_size = w * h * d * 4  # float32
            self.volumes.append({
                'offset': current_offset + 12,
                'dims': (w, h, d),
                'size': data_size
            })

            current_offset += 12 + data_size

    def __len__(self):
        return self.num_volumes

    def __getitem__(self, idx):
        vol_info = self.volumes[idx]

        # Pre-allocate NumPy array
        w, h, d = vol_info['dims']
        volume = np.empty(w * h * d, dtype=np.float32)

        # Zero-copy read into NumPy array
        volume_bytes = volume.view(np.uint8)
        self.reader.seek(vol_info['offset'])
        self.reader.read(buffer=volume_bytes)

        # Reshape to 3D
        volume = volume.reshape(d, h, w)  # DHW format

        if self.transform:
            volume = self.transform(volume)

        return torch.from_numpy(volume)


# Usage
dataset = BrainMRIStrata("s3://medical-data/brain-mri.st")
loader = torch.utils.data.DataLoader(
    dataset,
    batch_size=4,
    num_workers=2
)

for batch in loader:
    # batch shape: [4, D, H, W]
    print(batch.shape)
```

## Example 3: Text Datasets — Wikipedia

Efficiently storing and loading large text corpora.

```python
"""
Wikipedia articles stored in Strata.

Format:
  [8 bytes: num_articles]
  [num_articles * 16 bytes: (offset, length) pairs]
  [Article data: UTF-8 text back-to-back]
"""

import strata
import numpy as np
from torch.utils.data import IterableDataset

class WikipediaStrata(IterableDataset):
    def __init__(self, snapshot_path, tokenizer, max_length=512):
        self.reader = strata.open(snapshot_path)
        self.tokenizer = tokenizer
        self.max_length = max_length

        # Read index
        num_articles_bytes = self.reader.read(8, offset=0)
        self.num_articles = int.from_bytes(num_articles_bytes, 'little')

        index_size = self.num_articles * 16
        index_bytes = self.reader.read(index_size, offset=8)
        self.index = np.frombuffer(index_bytes, dtype=np.uint64).reshape(-1, 2)

        self.data_offset = 8 + index_size

    def __iter__(self):
        # For IterableDataset, yield items one by one
        # This enables shuffling and multi-worker support
        worker_info = torch.utils.data.get_worker_info()

        if worker_info is None:
            # Single-process loading
            start, end = 0, self.num_articles
        else:
            # Multi-process: split data among workers
            per_worker = int(np.ceil(self.num_articles / worker_info.num_workers))
            start = worker_info.id * per_worker
            end = min(start + per_worker, self.num_articles)

        for idx in range(start, end):
            offset, length = self.index[idx]
            actual_offset = self.data_offset + offset

            # Read article text
            text_bytes = self.reader.read(int(length), offset=actual_offset)
            text = text_bytes.decode('utf-8')

            # Tokenize
            tokens = self.tokenizer(
                text,
                max_length=self.max_length,
                truncation=True,
                padding='max_length',
                return_tensors='pt'
            )

            yield {
                'input_ids': tokens['input_ids'].squeeze(0),
                'attention_mask': tokens['attention_mask'].squeeze(0)
            }


# Usage with Hugging Face transformers
from transformers import BertTokenizer, BertForMaskedLM, AdamW
from torch.utils.data import DataLoader

tokenizer = BertTokenizer.from_pretrained('bert-base-uncased')
dataset = WikipediaStrata(
    "s3://nlp-data/wikipedia.st",
    tokenizer=tokenizer
)

loader = DataLoader(
    dataset,
    batch_size=32,
    num_workers=4
)

model = BertForMaskedLM.from_pretrained('bert-base-uncased')
optimizer = AdamW(model.parameters(), lr=5e-5)

model.train()
for batch in loader:
    outputs = model(**batch, labels=batch['input_ids'])
    loss = outputs.loss
    loss.backward()
    optimizer.step()
    optimizer.zero_grad()
```

## Example 4: Creating Snapshots from Raw Data

Programmatically pack datasets into Strata format.

```python
"""
Create ImageNet snapshot from directory of images.
"""

import strata
import os
from PIL import Image
import io
import struct

def pack_imagenet(source_dir, output_snapshot):
    """
    Pack ImageNet JPEG files into Strata snapshot.

    Args:
        source_dir: Path to directory with train/n01440764/*.JPEG structure
        output_snapshot: Output .st file path
    """
    # Collect all image paths
    image_paths = []
    for class_dir in sorted(os.listdir(source_dir)):
        class_path = os.path.join(source_dir, class_dir)
        if not os.path.isdir(class_path):
            continue

        for img_file in sorted(os.listdir(class_path)):
            if img_file.endswith('.JPEG'):
                image_paths.append(os.path.join(class_path, img_file))

    print(f"Found {len(image_paths)} images")

    # Write to temporary file that will be packed
    temp_file = output_snapshot + ".tmp"

    with open(temp_file, 'wb') as f:
        # Write number of images
        f.write(struct.pack('<Q', len(image_paths)))

        # Reserve space for index (will fill later)
        index_start = f.tell()
        f.write(b'\x00' * (len(image_paths) * 16))

        # Write images and build index
        index = []
        data_start = f.tell()

        for img_path in image_paths:
            offset = f.tell() - data_start

            # Read JPEG file
            with open(img_path, 'rb') as img_f:
                jpeg_data = img_f.read()

            length = len(jpeg_data)
            f.write(jpeg_data)

            index.append((offset, length))

        # Write index
        f.seek(index_start)
        for offset, length in index:
            f.write(struct.pack('<QQ', offset, length))

    # Pack the temporary file into Strata snapshot
    strata.pack(
        output=output_snapshot,
        disk=temp_file,
        compression="lz4",  # Fast for training
        block_size=65536
    )

    # Clean up
    os.remove(temp_file)
    print(f"Created snapshot: {output_snapshot}")


# Usage
pack_imagenet(
    source_dir="/data/imagenet/train",
    output_snapshot="imagenet-train.st"
)
```

## Example 5: Incremental Dataset Updates

Create delta snapshots for dataset versioning.

```python
"""
Create incremental snapshots when adding new samples.
"""

import strata

def create_base_snapshot(data_dir, output):
    """Create initial snapshot."""
    strata.pack(
        output=output,
        disk=data_dir,
        compression="zstd",
        cdc=True,  # Enable content-defined chunking for dedup
        block_size=65536
    )


def create_delta_snapshot(base_snapshot, new_data_dir, output):
    """
    Create delta snapshot with only new/changed data.

    This requires CLI tools since Python API doesn't expose
    overlay merge yet. See CLI documentation for:
      strata data pack --base <base> --overlay <new_data> --thin
    """
    # This would be done via CLI in practice
    import subprocess

    subprocess.run([
        'strata', 'data', 'pack',
        '--base', base_snapshot,
        '--overlay', new_data_dir,
        '--output', output,
        '--thin',  # Only store differences
        '--compression', 'zstd',
        '--cdc'
    ], check=True)


# Example workflow
create_base_snapshot("dataset-v1", "dataset-v1.st")

# Later: add more data
create_delta_snapshot(
    base_snapshot="dataset-v1.st",
    new_data_dir="dataset-v2-delta",
    output="dataset-v2.st"
)

# Train on v2 (transparently reads from v1 for unchanged data)
reader = strata.open("dataset-v2.st")
```

## Example 6: Multi-Modal Datasets

Combining images, text, and metadata.

```python
"""
COCO-style dataset with images and captions.

Format:
  [8 bytes: num_samples]
  [num_samples * 32 bytes: index entries]
    [8 bytes: image_offset]
    [8 bytes: image_length]
    [8 bytes: caption_offset]
    [8 bytes: caption_length]
  [Image data: JPEG files]
  [Caption data: UTF-8 text]
"""

import strata
import numpy as np
from torch.utils.data import Dataset
from PIL import Image
import io

class COCOStrata(Dataset):
    def __init__(self, snapshot_path, transform=None, tokenizer=None):
        self.reader = strata.open(snapshot_path)
        self.transform = transform
        self.tokenizer = tokenizer

        # Read index
        num_samples_bytes = self.reader.read(8, offset=0)
        self.num_samples = int.from_bytes(num_samples_bytes, 'little')

        index_size = self.num_samples * 32
        index_bytes = self.reader.read(index_size, offset=8)
        self.index = np.frombuffer(index_bytes, dtype=np.uint64).reshape(-1, 4)

        self.data_offset = 8 + index_size

    def __len__(self):
        return self.num_samples

    def __getitem__(self, idx):
        img_off, img_len, cap_off, cap_len = self.index[idx]

        # Read image
        img_bytes = self.reader.read(
            int(img_len),
            offset=self.data_offset + img_off
        )
        image = Image.open(io.BytesIO(img_bytes)).convert('RGB')

        if self.transform:
            image = self.transform(image)

        # Read caption
        cap_bytes = self.reader.read(
            int(cap_len),
            offset=self.data_offset + cap_off
        )
        caption = cap_bytes.decode('utf-8')

        if self.tokenizer:
            caption = self.tokenizer(caption, return_tensors='pt')

        return image, caption


# Usage for image captioning
from transformers import CLIPProcessor, CLIPModel

processor = CLIPProcessor.from_pretrained("openai/clip-vit-base-patch32")
dataset = COCOStrata(
    "coco-train.st",
    transform=processor.image_processor,
    tokenizer=processor.tokenizer
)

# Train vision-language model...
```

## Performance Best Practices

### 1. Choose the Right Block Size

```python
# Small blocks (16KB): Better random access, worse compression
strata.pack(output="random-access.st", disk="data", block_size=16384)

# Large blocks (256KB): Better compression, slower random access
strata.pack(output="sequential.st", disk="data", block_size=262144)

# Sweet spot for most ML workloads: 64KB
strata.pack(output="balanced.st", disk="data", block_size=65536)
```

### 2. Optimize DataLoader Settings

```python
# CPU-bound (decompression): Increase workers
loader = DataLoader(dataset, num_workers=8)

# I/O-bound (S3): Increase prefetch
loader = DataLoader(dataset, num_workers=4, prefetch_factor=4)

# Memory-constrained: Reduce workers and prefetch
loader = DataLoader(dataset, num_workers=2, prefetch_factor=2)
```

### 3. Local Caching for Remote Datasets

```python
import os
import shutil

def get_cached_reader(s3_path, cache_dir="/tmp/strata-cache"):
    """Download snapshot to local cache if not present."""
    os.makedirs(cache_dir, exist_ok=True)

    cache_path = os.path.join(cache_dir, os.path.basename(s3_path))

    if not os.path.exists(cache_path):
        print(f"Downloading {s3_path} to {cache_path}...")
        # Use AWS CLI for faster download
        import subprocess
        subprocess.run(['aws', 's3', 'cp', s3_path, cache_path], check=True)

    return strata.open(cache_path)


# Usage
reader = get_cached_reader("s3://bucket/dataset.st")
```

## Next Steps

- [API Reference](README.md#api-reference) — Complete API documentation
- [CLI Guide](../cli/README.md) — Creating snapshots with command-line tools
- [Performance Tuning](../../BENCHMARKS.md) — Optimization strategies
