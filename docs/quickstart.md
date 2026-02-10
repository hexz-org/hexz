# Quick Start: 5 Minutes to First Result

Get from zero to reading data from a Strata snapshot in a few minutes. **All commands below are run from the repository root**; the **Makefile** is the single entry point (run **`make help`** for the full list).

## 1. Install

Strata is in development. Build the Python package from source:

```bash
git clone https://github.com/Alethic-Systems/strata.git
cd strata

# Build and install the Python package (from repo root)
make develop
```

Optional: install the CLI for packing from the command line:

```bash
make rust
# Binary: target/release/strata
```

## 2. Create a snapshot

**Option A — Python only (easiest):**

```python
import strata

# Write a small file, then pack it into a .st snapshot
with open("/tmp/hello.bin", "wb") as f:
    f.write(b"Hello, Strata! " * 64)

with strata.open("/tmp/hello.st", mode="w", compression="lz4") as w:
    w.add("/tmp/hello.bin")
```

**Option B — CLI:**

```bash
echo "Hello, Strata!" > /tmp/hello.txt
strata data pack --disk /tmp/hello.txt --output /tmp/hello.st --compression lz4
```

## 3. Open and read (first result)

```python
import strata

with strata.open("/tmp/hello.st") as reader:
    data = reader.read(64)
print(data)  # b'Hello, Strata! Hello, Strata! ...'
```

Or run the bundled example (from repo root):

```bash
python examples/quickstart.py
```

You should see the snapshot built, **original vs .st file size** (so you can see the compression benefit), and the first bytes printed.

## 4. Next steps

- **Folders:** Use `strata.build("my_data/", "dataset.st", profile="ml")` to pack a directory.
- **ML training:** Use `strata.Dataset("dataset.st", item_size=1024)` with `torch.utils.data.DataLoader`.
- **CLI:** See [CLI usage](usage/cli/README.md) for `strata data pack`, `strata sys keygen`, signing, and more.

## Summary

| Step        | Python        | CLI         |
|------------|----------------|-------------|
| Install    | `make develop` | `make rust` |
| Create .st | `strata.open(path, mode="w")` + `writer.add(...)` or `strata.build(dir, out)` | `strata data pack --disk <file> --output out.st` |
| Read       | `strata.open(path)` then `reader.read(n)` or `reader.iter_chunks()` | (use Python or `strata vm mount` for access) |

That’s it. You’ve created a snapshot and read from it.
