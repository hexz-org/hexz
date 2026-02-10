# Strata v0.1.0-alpha Release Notes

**Status:** Ready for Alpha Testing
**Release Date:** 2026-02-10

## Working Features (Ready to Test)

### Core Engine
- **Compression:** Full support for LZ4 (fast) and Zstd (high ratio) with dictionary training.
- **Deduplication:** Content-Defined Chunking (CDC) is implemented in `strata pack`.
- **Encryption:** AES-256-GCM encryption is supported in the packing pipeline.
- **Index:** Two-level hierarchical index (Master -> Pages) supports efficient random access.

### CLI Tools
- **`strata pack`**: Create snapshots from disk/memory images with full config (dedup, compression).
- **`strata boot`**: Boot QEMU VMs directly from snapshots with copy-on-write overlays.
- **`strata commit`**: Save overlay changes back to a new snapshot (supports thin provisioning!).
- **`strata mount`**: FUSE filesystem to mount snapshots as read-only folders.

### Python Integration
- **`strata.Dataset`**: Fully functional PyTorch dataset with:
  - Multithreaded prefetching
  - LRU Caching
  - Shuffling support
- **`strata.pack()`**: Python binding for the high-performance Rust packer.

## Known Limitations (v0.1.0-alpha)

1.  **Metadata Storage**:
    - The format currently supports a simple "commit message" string via `strata commit`.
    - **Arbitrary Key-Value Metadata** (e.g., `writer.add_metadata({"accuracy": 0.95})`) is **NOT** yet persisted to disk. It is stored in-memory only.

2.  **Python `Writer` vs `pack`**:
    - `strata.Writer` (streaming API) does **not** yet support deduplication or encryption.
    - **Workaround:** Use `strata.pack()` for full feature support.

3.  **Backend Support**:
    - `boot`: QEMU works perfectly. **Firecracker** support is currently a stub (returns error).
    - `TFDataset`: TensorFlow integration is planned for v0.2.0.

## Testing Focus

We need feedback on:
1.  **Performance:** Read speeds in `strata.Dataset` vs native file loading.
2.  **Reliability:** Booting VMs from `strata mount` over long periods.
3.  **Deduplication:** Savings ratio on real-world datasets.
