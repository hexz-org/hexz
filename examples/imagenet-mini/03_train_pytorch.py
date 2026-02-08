# examples/imagenet-mini/03_train_pytorch.py
import torch
import torch.nn as nn
from torch.utils.data import DataLoader

from strata import StrataDataset

# 1. Define the Dataset
# We point it to the local file (or S3 URL)
# shuffle=True tells the Rust engine to randomize the block fetch order
dataset = StrataDataset(
    path="./imagenet-mini.st",
    shuffle=True,
    cache_size_mb=512,  # Keep 512MB of hot data in RAM
)

# 2. Wrap in standard PyTorch Loader
# num_workers=4: Spawns 4 python processes, but Strata manages the underlying Rust threads
loader = DataLoader(dataset, batch_size=32, num_workers=4)

# 3. Dummy Model
model = (
    nn.Sequential(
        nn.Conv2d(3, 16, 3), nn.ReLU(), nn.Flatten(), nn.Linear(16 * 62 * 62, 10)
    ).cuda()
    if torch.cuda.is_available()
    else nn.Sequential()
)

print("Starting Training Loop...")

# 4. The Loop (This is where speed matters)
for epoch in range(3):
    for i, batch in enumerate(loader):
        # 'batch' is already decoded and ready
        images, labels = batch

        # Simulate training work
        if torch.cuda.is_available():
            images = images.cuda()

        if i % 100 == 0:
            print(f"Epoch {epoch} | Batch {i} | Loaded {len(images)} images via Strata")

print("Training finished successfully!")
