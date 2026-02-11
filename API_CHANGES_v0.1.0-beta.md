# Python API Changes in v0.1.0-beta

## Summary

The Python API has been significantly cleaned up and simplified, reducing the public API surface from **48 items to 30 items** (38% reduction). The API is now more Pythonic, consistent, and easier to discover.

## Breaking Changes

### Removed Functions

The following top-level functions have been **removed** (not deprecated):

| Old Function | New Alternative | Reason |
|--------------|----------------|--------|
| `strata.info(path)` | `print(strata.inspect(path))` or `strata.inspect(path).print()` | Consolidate into Metadata |
| `strata.analyze(path)` | `reader.analyze()` | Move to Reader method |
| `strata.diff(path1, path2)` | `strata.Metadata.diff(path1, path2)` | Move to Metadata classmethod |
| `strata.merge_overlay(...)` | `writer.merge_overlay(...)` | Move to Writer method |
| `strata.unmount(path)` | Use `with strata.mount(...) as mp:` | Context manager handles cleanup |
| `strata.pack(...)` | `strata.build(...)` or `Writer` | Redundant with high-level APIs |
| `strata.snapshot_vm(...)` | CLI only | Better suited for CLI tool |

### Moved to Submodules

Cryptographic functions moved to `strata.crypto`:

| Old Function | New Location |
|--------------|--------------|
| `strata.keygen(...)` | `strata.crypto.keygen(...)` |
| `strata.sign_image(...)` | `strata.crypto.sign(...)` |
| `strata.verify_image(...)` | `strata.crypto.verify(...)` or `strata.verify(..., public_key=...)` |

### Internal Classes

The following classes are now internal (prefixed with `_`):

- `MountPoint` → `_MountPoint` (use `strata.mount()` function instead)

### Type Exports Reduced

Only commonly-used types are exported:

- **Kept**: `PathLike`, `Metadata`
- **Removed from exports** (still accessible via `strata.typing`): `Shape`, `PackingMode`, `BuildProfile`, `DeduplicationMode`, `CompressionAlgorithm`, `AnalysisReport`

---

## New Features

### Enhanced Metadata Class

The `Metadata` class now includes:

```python
# Human-readable output
meta = strata.inspect("snapshot.st")
print(meta)  # Formatted output
meta.print()  # Same as above

# Diff as classmethod
diff = strata.Metadata.diff("old.st", "new.st")
```

### Reader.analyze() Method

Deduplication analysis is now a Reader method:

```python
with strata.open("snapshot.st") as reader:
    report = reader.analyze()
    print(f"Dedup savings: {report.savings_percent:.1f}%")
```

### Writer.merge_overlay() Method

Overlay merging is now a Writer method:

```python
with strata.open("merged.st", mode="w") as writer:
    writer.merge_overlay(
        base="base.st",
        overlay="overlay.img",
        thin=True
    )
```

### Crypto Submodule

Signing and verification organized under `strata.crypto`:

```python
from strata import crypto

crypto.keygen("snapshot.key", "snapshot.pub")
crypto.sign("snapshot.st", "snapshot.key")
if crypto.verify("snapshot.st", "snapshot.pub"):
    print("Valid signature!")
```

---

## Migration Guide

### Quick Migration Examples

```python
# OLD (v0.1.0-alpha)
strata.info("file.st")
# NEW (v0.1.0-beta)
print(strata.inspect("file.st"))

# OLD
report = strata.analyze("file.st")
# NEW
with strata.open("file.st") as reader:
    report = reader.analyze()

# OLD
diff = strata.diff("a.st", "b.st")
# NEW
diff = strata.Metadata.diff("a.st", "b.st")

# OLD
strata.merge_overlay("base.st", "overlay.img", "out.st")
# NEW
with strata.open("out.st", mode="w") as writer:
    writer.merge_overlay(base="base.st", overlay="overlay.img")

# OLD
strata.keygen("key.priv", "key.pub")
# NEW
from strata import crypto
crypto.keygen("key.priv", "key.pub")

# OLD
strata.unmount("/mnt/point")
# NEW
# Use context manager - automatic cleanup
with strata.mount("snap.st") as mp:
    # use mp.path
    pass
# Auto-unmounted here
```

---

## Final Public API (30 items)

### Core I/O (5)
- `open()`
- `version()`
- `Reader`
- `AsyncReader`
- `Writer`

### ML Integration (2)
- `Dataset`
- `TFDataset`

### Arrays (3)
- `read_array()`
- `write_array()`
- `ArrayView`

### Build (2)
- `build()`
- `PROFILES`

### Inspection (1)
- `inspect()`

### Mount (1)
- `mount()`

### Verify (1)
- `verify()`

### Submodules (1)
- `crypto` (keygen, sign, verify)

### Types (2)
- `Metadata`
- `PathLike`

### Version Constants (3)
- `FORMAT_VERSION`
- `MIN_SUPPORTED_VERSION`
- `MAX_SUPPORTED_VERSION`

### Exceptions (10)
- `StrataError`
- `IOError`
- `NetworkError`
- `FormatError`
- `ValidationError`
- `CompressionError`
- `EncryptionError`
- `MountError`
- `CacheError`
- `VersionError`

---

## Performance Impact

**No performance regressions** introduced. All changes are structural reorganization with identical underlying implementations.

- Read/write performance: Unchanged
- Memory usage: Unchanged (actually slightly reduced due to less import overhead)
- API overhead: Minimal (method calls vs function calls are equivalent)

---

## Benefits

1. **Cleaner API**: 38% fewer top-level exports
2. **Better Organization**: Related functions grouped (crypto, Metadata methods)
3. **More Pythonic**: Context managers, property access, method chaining
4. **Easier Discovery**: Less cluttered namespace, clearer intent
5. **Type Safety**: Better IDE autocomplete and type checking
6. **Consistency**: Similar operations grouped together

---

## Compatibility

This is a **breaking change** from v0.1.0-alpha. Since we're in beta, we prioritized a clean API over backward compatibility. Users upgrading from alpha will need to migrate their code using the guide above.

Future changes (post-1.0) will maintain backward compatibility with deprecation warnings.
