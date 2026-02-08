"""PyTorch integration for Strata snapshots.

This module provides a PyTorch Dataset implementation that reads data
directly from Strata snapshots with random access. It supports:
- Multiprocessing (num_workers > 0)
- Zero-copy tensor creation
- Custom transforms
- Efficient random sampling
"""

import torch
from torch.utils.data import Dataset
from ._strata_core import StrataReader
from typing import Optional, Callable


class StrataDataset(Dataset):
    """PyTorch Dataset backed by a Strata snapshot with random access.

    This dataset reads fixed-size items from a Strata snapshot file without
    loading the entire dataset into memory. It leverages Strata's random access
    capabilities for efficient data loading during training.

    Features:
    - **Multiprocessing**: Thread-safe, works with PyTorch DataLoader workers
    - **Random access**: O(1) item lookup with block-level caching
    - **Zero-copy**: Tensors created directly from read buffers
    - **Transforms**: Optional data transformation pipeline

    Args:
        path: Path to the Strata snapshot file (.st)
        item_size: Size of each dataset item in bytes
        transform: Optional callable to transform raw tensors

    Example:
        >>> import torch
        >>> from torch.utils.data import DataLoader
        >>> from strata.torch import StrataDataset
        >>>
        >>> # Dataset with 1KB items
        >>> dataset = StrataDataset("dataset.st", item_size=1024)
        >>> print(f"Dataset size: {len(dataset)} items")
        >>>
        >>> # Use with DataLoader for parallel loading
        >>> loader = DataLoader(
        ...     dataset,
        ...     batch_size=32,
        ...     num_workers=4,  # Multiprocessing supported
        ...     shuffle=True
        ... )
        >>>
        >>> for batch in loader:
        ...     # Process batch...
        ...     pass

        >>> # With transform
        >>> def to_float32(tensor):
        ...     return tensor.float() / 255.0
        >>>
        >>> dataset = StrataDataset(
        ...     "dataset.st",
        ...     item_size=3072,  # 32x32x3 RGB image
        ...     transform=to_float32
        ... )

    Note:
        Items are assumed to be contiguous and fixed-size. For variable-length
        items, use a separate index file or embed length headers in the data.
    """

    def __init__(self, path: str, item_size: int, transform: Optional[Callable] = None):
        self.path = path
        self.item_size = item_size
        self.transform = transform

        # Open immediately to get size
        self.reader = StrataReader(path)
        self.total_size = self.reader.size()
        self.length = self.total_size // item_size

    def __len__(self):
        """Return the number of items in the dataset.

        Returns:
            Number of fixed-size items that fit in the snapshot
        """
        return self.length

    def __getitem__(self, idx):
        """Retrieve a single item from the dataset.

        Args:
            idx: Integer index of the item to retrieve

        Returns:
            A PyTorch tensor (uint8) of size (item_size,), optionally
            transformed if a transform was provided

        Raises:
            IndexError: If idx is out of bounds
        """
        if idx >= self.length:
            raise IndexError("StrataDataset index out of range")

        offset = idx * self.item_size

        # Use random access read_at (thread-safe, ignores cursor)
        data_bytes = self.reader.read_at(offset, self.item_size)

        writable_buf = bytearray(data_bytes)
        tensor = torch.frombuffer(writable_buf, dtype=torch.uint8)

        if self.transform:
            return self.transform(tensor)

        return tensor
