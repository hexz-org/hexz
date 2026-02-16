# Hexz Examples

Modular examples demonstrating key features of the Hexz Python API. **All commands assume you are in the repository root** and have run **`make develop`** at least once (see the main [README](../README.md) and **`make help`**).

**Reading from snapshots:** Use a single `read()` API. Open with `hexz.open(path)` to get a `Reader`, then:
- `reader.read(n)` — read `n` bytes from current position (advances cursor).
- `reader.read(n, offset=k)` — read `n` bytes at offset `k` (cursor unchanged).
- `reader.read(buffer=buf)` — fill a buffer from cursor; `reader.read(buffer=buf, offset=k)` for offset.
- `reader.iter_chunks(chunk_size=...)` — iterate in fixed-size chunks with one reused buffer.

## 1. Quick Start (`quickstart.py`)

**Run first.** Creates a tiny snapshot and reads it back — no CLI or extra data required.

```bash
python examples/quickstart.py
```

See [docs/quickstart.md](../docs/quickstart.md) for the full 5-minute guide.

## 2. Build Profiles (`build_profiles.py`)

Demonstrates `hexz.build()` with custom profile overrides (`archival`, `ml`, `eda`) for fine-tuned block size and compression.

```bash
python examples/build_profiles.py
```

## 3. Dataset Creation (`create_dataset.py`)

Generates a variable-length dataset (`dataset.hxz`) with an index file (`dataset.idx`), ready for use with the training examples.

```bash
python examples/create_dataset.py
```

## 4. PyTorch Training (`train_pytorch.py`)

Loads a Hexz dataset using `hexz.Dataset` (with caching and prefetching) via `torch.utils.data.DataLoader`.

**Requires:** PyTorch (`pip install torch`)

```bash
python examples/create_dataset.py   # generate data first
python examples/train_pytorch.py
```

## 5. MNIST Training (`mnist_training.py`)

Complete end-to-end MNIST training pipeline: download, pack into Hexz, train a CNN.

**Requires:** PyTorch, torchvision

```bash
python examples/mnist_training.py
```
