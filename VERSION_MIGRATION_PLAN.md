# Strata Format Migration Strategy

## Overview

This document describes the strategy for migrating Strata files between format versions. The goal is to provide a seamless upgrade path while maintaining backward compatibility.

---

## Migration API Design

### Python API

```python
import strata

# Check if migration is needed
info = strata.inspect("old.st")
if info.version < strata.FORMAT_VERSION:
    print(f"File is version {info.version}, current is {strata.FORMAT_VERSION}")
    print("Migration recommended")

# Migrate to latest version
strata.migrate("old_v1.st", "new_v2.st")

# Migrate to specific version
strata.migrate("old_v1.st", "new_v2.st", target_version=2)

# In-place migration (risky but convenient)
strata.migrate("file.st", overwrite=True)

# Dry-run to see what would change
changes = strata.migrate("old.st", "new.st", dry_run=True)
print(f"Would add: {changes.added_features}")
print(f"Would update: {changes.modified_blocks}")
```

### CLI

```bash
# Check version
strata info file.st

# Migrate to latest
strata migrate old_v1.st new_v2.st

# Migrate to specific version
strata migrate old_v1.st new_v2.st --target-version 2

# In-place migration
strata migrate file.st --in-place

# Dry-run
strata migrate old.st new.st --dry-run
```

---

## Migration Strategies

### Strategy 1: Copy-on-Migrate (Recommended)

**Approach:** Read from old file, write to new file with new format

**Advantages:**
- ✅ Safe - original file untouched
- ✅ Simple to implement
- ✅ Can add new features during migration
- ✅ Can optimize layout (re-dedup, re-compress)

**Disadvantages:**
- ❌ Requires 2x disk space temporarily
- ❌ Slow for large files (full copy)

**Implementation:**
```python
def migrate(
    source: PathLike,
    dest: PathLike,
    *,
    target_version: Optional[int] = None,
    optimize: bool = False,
) -> MigrationResult:
    """Migrate by copying to new file."""
    # Read old file
    reader = Reader(source)
    old_version = reader.metadata.version

    # Determine target version
    if target_version is None:
        target_version = FORMAT_VERSION

    # Create new file with target version
    with Writer(dest, format_version=target_version) as writer:
        # Copy data blocks
        for chunk in reader.iter_chunks(1024 * 1024):
            writer.add_bytes(chunk)

        # Migrate metadata
        old_meta = reader.metadata
        new_meta = _migrate_metadata(old_meta, target_version)
        writer.add_metadata(new_meta)

        # Add new features based on target version
        if target_version >= 2:
            # V2 adds block checksums
            writer.enable_checksums()

    return MigrationResult(
        old_version=old_version,
        new_version=target_version,
        bytes_migrated=reader.size,
    )
```

**Use Cases:**
- Upgrading major versions (v1 → v2)
- Adding new compression algorithms
- Re-optimizing old files
- Adding encryption to unencrypted files

---

### Strategy 2: In-Place Header Update (Fast)

**Approach:** Update only the header, leave data blocks unchanged

**Advantages:**
- ✅ Very fast (seconds even for TB files)
- ✅ No extra disk space needed
- ✅ Preserves block layout

**Disadvantages:**
- ❌ Risky - corrupts file on failure
- ❌ Can't add features that require data changes
- ❌ Limited to header-only changes

**Implementation:**
```python
def migrate_in_place(
    path: PathLike,
    *,
    target_version: int,
    backup: bool = True,
) -> MigrationResult:
    """Migrate by updating header only."""
    # Safety check
    if not _can_migrate_in_place(path, target_version):
        raise ValueError(
            "In-place migration not possible - "
            "requires data structure changes. Use copy-on-migrate instead."
        )

    # Create backup if requested
    if backup:
        shutil.copy2(path, f"{path}.backup")

    try:
        # Open file for read-write
        with open(path, "r+b") as f:
            # Read old header
            header_bytes = f.read(HEADER_SIZE)
            old_header = deserialize_header(header_bytes)

            # Update header
            new_header = old_header.copy()
            new_header.version = target_version
            new_header.features = _upgrade_features(
                old_header.features, target_version
            )

            # Write new header
            f.seek(0)
            f.write(serialize_header(new_header))
            f.flush()

        # Remove backup on success
        if backup:
            os.remove(f"{path}.backup")

        return MigrationResult(
            old_version=old_header.version,
            new_version=target_version,
            in_place=True,
        )

    except Exception as e:
        # Restore backup on failure
        if backup and os.path.exists(f"{path}.backup"):
            shutil.move(f"{path}.backup", path)
        raise
```

**Use Cases:**
- Minor version bumps (v1.0 → v1.1)
- Adding feature flags for existing features
- Updating metadata format (if stored separately)

---

### Strategy 3: Hybrid (Smart)

**Approach:** Use in-place for compatible changes, copy-on-migrate for incompatible

**Advantages:**
- ✅ Fast when possible
- ✅ Safe when needed
- ✅ Best of both worlds

**Implementation:**
```python
def migrate(
    source: PathLike,
    dest: Optional[PathLike] = None,
    *,
    target_version: Optional[int] = None,
    overwrite: bool = False,
) -> MigrationResult:
    """Smart migration - chooses strategy automatically."""
    if target_version is None:
        target_version = FORMAT_VERSION

    # Check if in-place is possible
    can_in_place = _can_migrate_in_place(source, target_version)

    if overwrite:
        if can_in_place:
            # Fast in-place update
            return migrate_in_place(source, target_version=target_version)
        else:
            # Copy to temp, then replace
            temp = f"{source}.migrating"
            result = migrate_copy(source, temp, target_version=target_version)
            shutil.move(temp, source)
            return result
    else:
        if dest is None:
            raise ValueError("Must specify dest or use overwrite=True")

        # Always safe to copy
        return migrate_copy(source, dest, target_version=target_version)
```

---

## Version-Specific Migrations

### V1 → V2 (Hypothetical)

**New features in V2:**
- Block-level CRC32 checksums
- Improved compression (Brotli support)
- Enhanced metadata format

**Migration paths:**

#### 1. Header-only (Fast)
If file doesn't need new features:
```python
# Just update version number
strata.migrate("v1.st", overwrite=True)  # < 1 second
```

#### 2. Add checksums (Medium)
Calculate checksums for existing blocks:
```python
# Read blocks, calculate CRC32, update index
strata.migrate("v1.st", "v2.st", add_checksums=True)  # ~1 GB/s
```

#### 3. Full re-compression (Slow)
Re-compress with Brotli:
```python
# Decompress, re-compress with new algorithm
strata.migrate(
    "v1.st",
    "v2.st",
    recompress="brotli",
    compression_level=9,
)  # ~100 MB/s
```

---

## Migration Compatibility Matrix

| From | To | In-Place? | Copy? | Notes |
|------|----|-----------| ------|-------|
| v1 | v2 | ✅ Yes | ✅ Yes | Header update only |
| v1 | v2 (with checksums) | ❌ No | ✅ Yes | Requires block updates |
| v1 | v2 (with Brotli) | ❌ No | ✅ Yes | Requires re-compression |
| v2 | v1 | ⚠️ Partial | ✅ Yes | Loses v2 features |

---

## Safety Features

### 1. Dry-Run Mode

```python
result = strata.migrate("old.st", "new.st", dry_run=True)
print(f"Would migrate {result.bytes} bytes")
print(f"Added features: {result.added_features}")
print(f"Removed features: {result.removed_features}")
print(f"Estimated time: {result.estimated_time_seconds}s")
```

### 2. Automatic Backup

```python
# Automatically creates .backup file
strata.migrate("file.st", overwrite=True)  # Creates file.st.backup
```

### 3. Verification

```python
# Verify after migration
result = strata.migrate("old.st", "new.st", verify=True)
assert result.verified
```

### 4. Progressive Migration

For large files, show progress:
```python
def progress_callback(bytes_migrated, total_bytes):
    percent = 100 * bytes_migrated / total_bytes
    print(f"Migrating: {percent:.1f}%")

strata.migrate(
    "huge.st",
    "huge_v2.st",
    progress=progress_callback,
)
```

---

## Rust Implementation

### Core Migration Function

```rust
// crates/core/src/migration/mod.rs

pub struct MigrationPlan {
    pub source_version: u32,
    pub target_version: u32,
    pub strategy: MigrationStrategy,
    pub requires_recompression: bool,
    pub requires_data_copy: bool,
}

pub enum MigrationStrategy {
    InPlace,           // Update header only
    CopyOnMigrate,     // Full copy
    Hybrid,            // Mix of both
}

pub fn plan_migration(
    source: &StrataHeader,
    target_version: u32,
) -> Result<MigrationPlan> {
    let can_in_place = check_in_place_compatibility(
        source.version,
        target_version,
    );

    let strategy = if can_in_place {
        MigrationStrategy::InPlace
    } else {
        MigrationStrategy::CopyOnMigrate
    };

    Ok(MigrationPlan {
        source_version: source.version,
        target_version,
        strategy,
        requires_recompression: needs_recompression(source, target_version),
        requires_data_copy: !can_in_place,
    })
}

pub fn migrate(
    source_path: &Path,
    dest_path: &Path,
    plan: &MigrationPlan,
) -> Result<MigrationResult> {
    match plan.strategy {
        MigrationStrategy::InPlace => migrate_in_place(source_path, plan),
        MigrationStrategy::CopyOnMigrate => migrate_copy(source_path, dest_path, plan),
        MigrationStrategy::Hybrid => migrate_hybrid(source_path, dest_path, plan),
    }
}
```

### Python Bindings

```rust
// crates/loader/src/py_interface/migration.rs

#[pyfunction]
#[pyo3(signature = (source, dest=None, target_version=None, overwrite=false, dry_run=false))]
pub fn migrate(
    source: String,
    dest: Option<String>,
    target_version: Option<u32>,
    overwrite: bool,
    dry_run: bool,
) -> PyResult<PyMigrationResult> {
    // Implementation
}

#[pyclass]
pub struct PyMigrationResult {
    #[pyo3(get)]
    pub old_version: u32,
    #[pyo3(get)]
    pub new_version: u32,
    #[pyo3(get)]
    pub bytes_migrated: u64,
    #[pyo3(get)]
    pub in_place: bool,
    #[pyo3(get)]
    pub verified: bool,
}
```

---

## Downgrade Strategy

### Can We Downgrade?

**General rule:** Downgrading loses new features

**Safe downgrades:**
- v2 → v1: If file doesn't use v2-only features
- Remove checksums (data unchanged)
- Remove unused feature flags

**Unsafe downgrades:**
- v2 → v1: If file uses Brotli compression (v1 doesn't support)
- v2 → v1: If file uses ChaCha20 encryption (v1 doesn't support)

**Implementation:**
```python
def migrate(source, dest, target_version):
    """Migrate up or down."""
    info = inspect(source)

    if target_version < info.version:
        # Downgrade
        if not _can_downgrade(info, target_version):
            raise ValueError(
                f"Cannot downgrade from v{info.version} to v{target_version} - "
                f"file uses features: {info.v2_only_features}"
            )

        warnings.warn(
            f"Downgrading from v{info.version} to v{target_version} - "
            f"will lose features: {info.v2_only_features}",
            UserWarning,
        )

    # Proceed with migration
```

---

## Testing Strategy

### Unit Tests

```python
def test_migrate_v1_to_v2_header_only():
    """Test fast header-only migration."""
    # Create v1 file
    with Writer("test_v1.st", format_version=1) as w:
        w.add_bytes(b"test data")

    # Migrate
    result = strata.migrate("test_v1.st", "test_v2.st", target_version=2)

    assert result.old_version == 1
    assert result.new_version == 2
    assert result.bytes_migrated > 0

    # Verify
    info = strata.inspect("test_v2.st")
    assert info.version == 2

def test_migrate_with_checksums():
    """Test migration that adds checksums."""
    # Create v1 file without checksums
    with Writer("test_v1.st", format_version=1) as w:
        w.add_bytes(b"test data" * 1000)

    # Migrate and add checksums
    result = strata.migrate(
        "test_v1.st",
        "test_v2.st",
        target_version=2,
        add_checksums=True,
    )

    # Verify checksums present
    info = strata.inspect("test_v2.st")
    assert info.features.has_checksums

def test_migrate_in_place():
    """Test in-place migration."""
    # Create test file
    with Writer("test.st", format_version=1) as w:
        w.add_bytes(b"test data")

    # Get original size
    original_size = os.path.getsize("test.st")

    # Migrate in-place
    result = strata.migrate("test.st", overwrite=True, target_version=2)

    assert result.in_place
    assert result.old_version == 1
    assert result.new_version == 2

    # Size should be same (header-only change)
    assert os.path.getsize("test.st") == original_size

def test_migrate_dry_run():
    """Test dry-run mode."""
    with Writer("test.st", format_version=1) as w:
        w.add_bytes(b"test data")

    result = strata.migrate("test.st", "out.st", dry_run=True)

    assert result.old_version == 1
    assert result.new_version == 2
    assert not os.path.exists("out.st")  # Not actually created
```

---

## Implementation Priority

### Phase 1: Core Infrastructure (2-3 hours)
1. Add `version.rs` module with compatibility checking
2. Add `MigrationPlan` struct
3. Basic in-place header update

### Phase 2: Python API (2-3 hours)
1. Add `migrate()` function
2. Add `MigrationResult` class
3. CLI command: `strata migrate`

### Phase 3: Safety Features (2 hours)
1. Automatic backups
2. Verification
3. Dry-run mode

### Phase 4: Advanced Features (4-6 hours)
1. Copy-on-migrate for incompatible changes
2. Progressive migration with progress callbacks
3. Downgrade support

**Total Effort:** 10-14 hours

---

## Recommendations

### For v0.3.0 Release:
1. ✅ Implement version range checking (CRITICAL)
2. ✅ Add basic `migrate()` function (copy-on-migrate)
3. ✅ Add dry-run and verification
4. ⚠️ Skip in-place migration for now (risky, less important)

### For v1.0.0 Release:
1. Add in-place migration for minor version bumps
2. Add downgrade support with safety checks
3. Add migration guide documentation
4. Add migration tests for all version pairs

### Best Practices:
1. **Always use copy-on-migrate by default** (safe)
2. **Require explicit --in-place flag** for in-place migration (risky)
3. **Always verify after migration** (catch corruption)
4. **Always create backup before overwrite** (safety net)
5. **Show clear warnings for downgrades** (data loss)

---

## API Examples

### Simple Migration

```python
import strata

# Check current version
info = strata.inspect("dataset.st")
print(f"Current version: {info.version}")
print(f"Latest version: {strata.FORMAT_VERSION}")

# Migrate to latest
if info.version < strata.FORMAT_VERSION:
    print("Migrating...")
    result = strata.migrate("dataset.st", "dataset_v2.st")
    print(f"Migrated {result.bytes_migrated / 1e9:.1f} GB")
    print(f"Old version: {result.old_version}")
    print(f"New version: {result.new_version}")
```

### Advanced Migration

```python
# Dry-run first
result = strata.migrate("data.st", "data_v2.st", dry_run=True)
print(f"Will migrate {result.bytes_migrated / 1e9:.1f} GB")
print(f"Estimated time: {result.estimated_seconds:.1f}s")
print(f"New features: {result.added_features}")

# Proceed if acceptable
if input("Continue? [y/n] ") == "y":
    result = strata.migrate(
        "data.st",
        "data_v2.st",
        verify=True,  # Verify after migration
        progress=lambda done, total: print(f"{100*done/total:.1f}%"),
    )
    print(f"Success! Verified: {result.verified}")
```

### In-Place Migration (Fast)

```python
# Only for compatible version bumps
result = strata.migrate(
    "data.st",
    overwrite=True,  # In-place update
    target_version=2,
)
if result.in_place:
    print("Fast in-place migration completed!")
else:
    print("Required full copy (incompatible changes)")
```

---

**Last Updated:** 2026-02-09
**Status:** Design complete, ready for implementation
