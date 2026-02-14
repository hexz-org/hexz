# Setup S3 Streaming for ML Workflows

**Goal**: Stream training data directly from S3 to GPU without downloading entire datasets to local disk.

**Prerequisites**:
- AWS account with S3 access
- Hexz Python package installed
- AWS CLI configured or credentials available

## Problem

Downloading large ML datasets (ImageNet, COCO, custom datasets) to local storage is slow, expensive, and wastes disk space. A 500GB dataset takes hours to download and requires 500GB of NVMe.

## Solution

Hexz streams compressed snapshots directly from S3, decompressing blocks on-demand as the DataLoader requests them. Only active blocks are cached locally (default 256MB cache).

## Step 1: Configure AWS Credentials

Hexz uses the standard AWS credential chain.

**Option A: AWS CLI (Recommended)**:
```bash
# Configure AWS CLI (one-time setup)
aws configure

# Enter your credentials:
# AWS Access Key ID: AKIA...
# AWS Secret Access Key: ...
# Default region: us-west-2
```

**Option B: Environment Variables**:
```bash
export AWS_ACCESS_KEY_ID="AKIA..."
export AWS_SECRET_ACCESS_KEY="..."
export AWS_DEFAULT_REGION="us-west-2"
```

**Option C: IAM Role (EC2/ECS)**:
No configuration needed. Hexz automatically uses instance metadata service (IMDS).

**Verify Access**:
```bash
# Test S3 access
aws s3 ls s3://your-bucket/
```

## Step 2: Upload Snapshot to S3

Pack your dataset and upload it:

```bash
# Pack dataset locally
hexz data pack \\
  --disk /data/imagenet-train \\
  --output imagenet-train.hxz \\
  --compression zstd \\
  --cdc

# Upload to S3
aws s3 cp imagenet-train.hxz s3://my-ml-datasets/imagenet-train.hxz

# Verify upload
aws s3 ls s3://my-ml-datasets/imagenet-train.hxz
```

**Tip**: Use `aws s3 cp --storage-class INTELLIGENT_TIERING` to automatically optimize storage costs.

## Step 3: Stream from S3 in Python

Open the snapshot using the S3 URL:

```python
import hexz
import torch
from torch.utils.data import DataLoader

# Open snapshot from S3 (downloads index only, ~1MB)
dataset = hexz.open("s3://my-ml-datasets/imagenet-train.hxz")

print(f"Dataset size: {dataset.size()} bytes")
print(f"Index downloaded, data will stream on-demand")

# Read sample
sample = dataset.read(4096, offset=0)
print(f"Read {len(sample)} bytes")
```

**What Happens**:
1. Hexz downloads the snapshot index (~1MB for 1TB dataset)
2. Index is cached in memory
3. Data blocks are fetched from S3 only when accessed
4. Recently used blocks cached in RAM (default 256MB LRU cache)

## Step 4: Configure S3 Region

**Specify Region Explicitly**:
```python
dataset = hexz.open(
    "s3://my-ml-datasets/imagenet-train.hxz",
    s3_region="us-west-2"  # Match your bucket region
)
```

**Why This Matters**: Reading from the wrong region adds 50-100ms latency per request.

**Check Bucket Region**:
```bash
aws s3api get-bucket-location --bucket my-ml-datasets
```

## Step 5: Optimize Cache Settings

**Increase Cache Size for Large Batches**:
```python
dataset = hexz.open(
    "s3://my-ml-datasets/imagenet-train.hxz",
    cache_size=1024 * 1024 * 1024  # 1GB cache (default 256MB)
)
```

**Enable Disk Cache for Multi-Epoch Training**:
```python
dataset = hexz.open(
    "s3://my-ml-datasets/imagenet-train.hxz",
    cache_dir="/tmp/hexz-cache",  # Persist cache to disk
    cache_size=2 * 1024**3  # 2GB
)
```

**Cache Behavior**:
- In-memory cache: Fast but lost on restart
- Disk cache: Persists across runs, speeds up subsequent epochs

## Step 6: Handle Connection Errors

S3 requests can fail due to network issues. Hexz retries automatically, but you can configure it:

```python
dataset = hexz.open(
    "s3://my-ml-datasets/imagenet-train.hxz",
    retry_attempts=5,  # Default 3
    retry_delay=2.0    # Seconds between retries (exponential backoff)
)
```

**Timeout Configuration**:
```python
dataset = hexz.open(
    "s3://my-ml-datasets/imagenet-train.hxz",
    connect_timeout=10,  # Connection timeout (seconds)
    read_timeout=30      # Read timeout (seconds)
)
```

## Step 7: Benchmark Performance

Compare local vs. S3 streaming:

```python
import time

# Local file
local_dataset = hexz.open("/nvme/imagenet-train.hxz")
start = time.time()
for i in range(1000):
    local_dataset.read(64*1024, offset=i*1024*1024)
local_time = time.time() - start
print(f"Local: {local_time:.2f}s")

# S3 streaming (cold cache)
s3_dataset = hexz.open("s3://my-ml-datasets/imagenet-train.hxz")
start = time.time()
for i in range(1000):
    s3_dataset.read(64*1024, offset=i*1024*1024)
s3_time = time.time() - start
print(f"S3 (cold): {s3_time:.2f}s")

# S3 streaming (warm cache)
start = time.time()
for i in range(1000):
    s3_dataset.read(64*1024, offset=i*1024*1024)
s3_warm_time = time.time() - start
print(f"S3 (warm): {s3_warm_time:.2f}s")
```

**Expected Results**:
- Local: 0.05s (NVMe)
- S3 (cold): 15.0s (network latency)
- S3 (warm): 0.08s (cache hit)

**Key Insight**: After the first epoch, cache hit rate is high, making S3 streaming nearly as fast as local.

## Complete Training Example

```python
import torch
from torch.utils.data import DataLoader
from torchvision import transforms
from hexz_dataset import ImageDataset  # From tutorial

# Open S3 dataset
transform = transforms.Compose([
    transforms.ToTensor(),
    transforms.Normalize(mean=[0.485, 0.456, 0.406],
                       std=[0.229, 0.224, 0.225])
])

dataset = ImageDataset(
    "s3://my-ml-datasets/imagenet-train.hxz",
    transform=transform,
    cache_size=2*1024**3,  # 2GB cache
    cache_dir="/tmp/hexz-cache"
)

# Standard DataLoader
loader = DataLoader(
    dataset,
    batch_size=32,
    shuffle=True,
    num_workers=4,  # Parallel workers improve S3 throughput
    pin_memory=True
)

# Train
for epoch in range(10):
    for batch_idx, (images, labels) in enumerate(loader):
        # Your training code here
        pass
```

## Troubleshooting

**"NoCredentialsError: Unable to locate credentials"**:
- Run `aws configure` or set `AWS_ACCESS_KEY_ID` environment variable
- Verify credentials: `aws sts get-caller-identity`

**"403 Forbidden"**:
- Check bucket permissions: User needs `s3:GetObject` permission
- Verify bucket policy allows access from your IP/VPC

**Slow performance (>500ms per read)**:
- Check bucket region matches `s3_region` parameter
- Increase `num_workers` in DataLoader (more parallel requests)
- Use larger `cache_size` to reduce repeated fetches

**"Connection timeout"**:
- Increase `connect_timeout` parameter
- Check network connectivity: `curl https://s3.amazonaws.com`

## Best Practices

1. **Region Locality**: Run training instances in the same region as your S3 bucket
2. **Bucket Optimization**: Use S3 Transfer Acceleration for cross-region access
3. **Cache Sizing**: Set cache size to 1-2× your batch memory footprint
4. **Worker Count**: Use `num_workers=4-8` to parallelize S3 requests
5. **Compression**: Use `zstd` (better ratio) for S3 to reduce bandwidth costs
6. **Monitoring**: Track S3 request metrics in CloudWatch

## Next Steps

- [Optimize PyTorch DataLoader Performance](optimize-pytorch-dataloader.md)
- [Performance Tuning Guide](../performance-tuning.md)
- [Migration from WebDataset](migrate-from-webdataset.md)

## See Also

- [Reference: Configuration Options](../../reference/configuration.md)
- [Explanation: Storage Backend Design](../../explanation/storage-backend-design.md)
