"""
Script to demonstrate training loop with Hexz Dataset.
"""

import time
import hexz
from torch.utils.data import DataLoader
from torch.nn.utils.rnn import pad_sequence


def collate_pad(batch):
    """
    Custom collate function to handle variable length items by padding them.
    batch is a list of tensors.
    """
    return pad_sequence(batch, batch_first=True, padding_value=0)


def train_loop():
    print("Initializing Hexz Dataset...")

    # Initialize dataset
    # We use the index file we created to handle variable length items
    dataset = hexz.Dataset(
        "dataset.hxz",
        index_file="dataset.idx",
        output_format="tensor",
        cache_size_mb=512,  # 512MB cache
        prefetch_factor=4,  # Prefetch 4 items ahead
        num_workers=2,  # 2 background threads for prefetching
        shuffle=True,  # Enable shuffling
        seed=42,
    )

    print(f"Dataset size: {len(dataset)} items")
    print(f"Cache size: {dataset.cache_stats()['size_mb']:.2f} MB")

    # Create DataLoader
    # Note: We set num_workers=0 because Hexz handles prefetching internally via threads.
    # Using multiprocessing (num_workers > 0 in DataLoader) works but duplicates the Hexz Dataset instance.
    loader = DataLoader(dataset, batch_size=32, num_workers=0, collate_fn=collate_pad)

    print("\nStarting training loop...")
    start_time = time.time()

    for epoch in range(3):
        print(f"\nEpoch {epoch + 1}/3")
        dataset.set_epoch(epoch)  # Important for DDP shuffling

        items_processed = 0

        for batch in loader:
            # batch is a Tensor of shape (batch_size, max_len_in_batch)
            # We just want to test data loading speed and access
            items_processed += len(batch)

        print(f"Processed {items_processed} items")
        stats = dataset.cache_stats()
        print(
            f"Cache stats: hits={stats['hits']}, misses={stats['misses']}, hit_rate={stats['hit_rate']:.2f}"
        )

    duration = time.time() - start_time
    print(f"\nTotal time: {duration:.2f}s")
    print(f"Throughput: {len(dataset) * 3 / duration:.1f} items/s")


if __name__ == "__main__":
    train_loop()
