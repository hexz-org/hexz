# Troubleshooting Guide

Common issues and their solutions for Hexz.

## Installation Issues

### "Rust compiler not found"

**Error**:
```
error: Rust compiler not found
```

**Solution**:
```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### "Python package 'hexz' not found"

**Error**:
```python
ModuleNotFoundError: No module named 'hexz'
```

**Solutions**:
1. Activate virtual environment: `source .venv/bin/activate`
2. Reinstall: `make develop`
3. Check Python version: `python --version` (requires 3.8+)

### "libfuse not found"

**Error**:
```
error: could not find libfuse
```

**Solution**:
```bash
# Ubuntu/Debian
sudo apt install libfuse-dev

# Fedora/RHEL
sudo dnf install fuse-devel

# macOS
brew install macfuse
```

## ML/Training Issues

### "NoCredentialsError: Unable to locate credentials"

**Error** when accessing S3:
```
NoCredentialsError: Unable to locate credentials
```

**Solutions**:
1. Configure AWS CLI: `aws configure`
2. Set environment variables:
   ```bash
   export AWS_ACCESS_KEY_ID="..."
   export AWS_SECRET_ACCESS_KEY="..."
   ```
3. Verify: `aws sts get-caller-identity`

See [Setup S3 Streaming](ml-workflows/setup-s3-streaming.md) for details.

### Slow DataLoader Performance

**Symptom**: Training slower than expected

**Diagnostics**:
```python
import time
start = time.time()
for batch_idx, batch in enumerate(loader):
    if batch_idx == 10:
        break
elapsed = time.time() - start
print(f"10 batches in {elapsed:.2f}s")
```

**Solutions**:
1. Increase `num_workers`: Try 4-8 workers
2. Increase cache size:
   ```python
   dataset = hexz.open(path, cache_size=1024*1024*1024)  # 1GB
   ```
3. Enable disk cache for multi-epoch:
   ```python
   dataset = hexz.open(path, cache_dir="/tmp/hexz-cache")
   ```
4. Check S3 region matches bucket region

See [Optimize PyTorch DataLoader](ml-workflows/optimize-pytorch-dataloader.md).

### "Connection timeout" on S3

**Error**:
```
ConnectionError: Connection timeout
```

**Solutions**:
1. Increase timeout:
   ```python
   dataset = hexz.open(path, connect_timeout=30, read_timeout=60)
   ```
2. Check network: `curl https://s3.amazonaws.com`
3. Verify bucket region: `aws s3api get-bucket-location --bucket BUCKET`

## VM Issues

### "Permission denied" on KVM

**Error**:
```
Could not access KVM kernel module: Permission denied
```

**Solution**:
```bash
# Add user to kvm group
sudo usermod -a -G kvm $(whoami)

# Apply changes
newgrp kvm  # Or log out and back in
```

### "FUSE: Device not found"

**Error**:
```
/dev/fuse: No such file or directory
```

**Solutions**:
1. Load kernel module: `sudo modprobe fuse`
2. Install FUSE:
   ```bash
   # Ubuntu/Debian
   sudo apt install fuse
   ```

### "Parent snapshot not found"

**Error**:
```
Error: Parent snapshot not found: /old/path/base.st
```

**Cause**: Thin snapshot references parent by absolute path, which moved.

**Solutions**:
1. Restore parent to original location
2. Convert to standalone snapshot:
   ```bash
   hexz vm mount thin.st /mnt --overlay full.img
   sudo umount /mnt
   hexz vm commit --base thin.st --overlay full.img --output standalone.st
   ```

## Compression/Packing Issues

### "Out of memory" during pack

**Error**:
```
fatal runtime error: out of memory
```

**Solutions**:
1. Reduce block size:
   ```bash
   hexz data pack --disk data/ --output out.st --block-size 16384
   ```
2. Disable CDC (uses less memory):
   ```bash
   hexz data pack --disk data/ --output out.st  # No --cdc flag
   ```
3. Pack in smaller batches

### "Corrupted snapshot"

**Error**:
```
CorruptionError: Checksum mismatch
```

**Diagnostics**:
```bash
# Verify snapshot integrity
hexz data info snapshot.st
```

**Causes**:
- Incomplete download (S3, HTTP)
- Disk corruption
- Interrupted pack operation

**Solutions**:
1. Re-download snapshot
2. Re-pack from source data
3. Check disk health: `smartctl -a /dev/sda`

## Performance Issues

### High CPU usage

**Symptom**: 100% CPU usage during training

**Causes**:
1. Too many DataLoader workers (context switching)
2. Zstd compression (CPU-intensive decompression)

**Solutions**:
1. Reduce workers: `num_workers=4` instead of 8
2. Use LZ4 for hot data:
   ```bash
   hexz data pack --disk data/ --output out.st --compression lz4
   ```

### High memory usage

**Symptom**: System running out of RAM

**Causes**:
- Large cache size
- Too many DataLoader workers
- Memory leak (report bug)

**Solutions**:
1. Reduce cache: `cache_size=256*1024*1024`  # 256MB
2. Reduce workers: `num_workers=2`
3. Monitor usage: `htop` or `top`

## Getting Help

**Still stuck?**

1. Run diagnostics: `hexz sys doctor`
2. Enable verbose logging:
   ```bash
   hexz -vvv data pack ...  # CLI
   ```
   ```python
   import logging
   logging.basicConfig(level=logging.DEBUG)  # Python
   ```
3. Check [GitHub Issues](https://github.com/Alethic-Systems/hexz/issues)
4. Open a new issue with:
   - Error message (full output)
   - OS and version
   - Hexz version: `hexz --version`
   - Steps to reproduce

## See Also

- [Performance Tuning](performance-tuning.md)
- [Reference: Configuration](../reference/configuration.md)
- [Contributing Guide](../project-docs/CONTRIBUTING.md)
