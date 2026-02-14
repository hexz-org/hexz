# Install Hexz

**Goal**: Install Hexz CLI and Python package on your system.

## Prerequisites

- Linux, macOS, or Windows
- Git installed
- Internet connection

## Installation Methods

### Method 1: Build from Source (Recommended)

**Step 1: Install Rust**:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

**Step 2: Clone Repository**:
```bash
git clone https://github.com/Alethic-Systems/hexz.git
cd hexz
```

**Step 3: Install System Dependencies**:

On Ubuntu/Debian:
```bash
sudo apt install pkg-config libfuse-dev python3-dev
```

On Fedora/RHEL:
```bash
sudo dnf install pkgconfig fuse-devel python3-devel
```

On macOS:
```bash
brew install macfuse pkg-config python3
```

**Step 4: Build CLI**:
```bash
make rust
```

Binary location: `./target/release/hexz`

**Step 5: Build Python Package**:
```bash
make develop
```

**Step 6: Verify Installation**:
```bash
./target/release/hexz --version
python -c "import hexz; print(hexz.__version__)"
```

### Method 2: Python Package Only

If you only need the Python loader (no CLI):

```bash
git clone https://github.com/Alethic-Systems/hexz.git
cd hexz
pip install maturin
maturin develop
```

### Method 3: Install CLI to System PATH

After building:

```bash
sudo cp ./target/release/hexz /usr/local/bin/
hexz --version
```

Or add to PATH:
```bash
echo 'export PATH="$HOME/hexz/target/release:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

## Verify Installation

### Check CLI

```bash
hexz --version
hexz sys doctor
```

### Check Python Package

```python
import hexz
print(hexz.__version__)

# Test basic functionality
with hexz.open("/tmp/test.hxz", mode="w") as writer:
    writer.write(b"Hello, Hexz!")

with hexz.open("/tmp/test.hxz") as reader:
    data = reader.read(14)
    assert data == b"Hello, Hexz!"
    print("Installation verified!")
```

## Optional Components

### QEMU (for VM features)

Ubuntu/Debian:
```bash
sudo apt install qemu-system-x86 qemu-utils
```

Fedora/RHEL:
```bash
sudo dnf install qemu-kvm qemu-img
```

macOS:
```bash
brew install qemu
```

### KVM (Linux only, for VM acceleration)

```bash
# Check KVM support
grep -E '(vmx|svm)' /proc/cpuinfo

# Add user to KVM group
sudo usermod -a -G kvm $(whoami)
newgrp kvm
```

### AWS CLI (for S3 features)

```bash
pip install awscli
aws configure
```

## Troubleshooting

### "Rust compiler not found"

Install Rust toolchain:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### "libfuse not found"

Install FUSE development headers:
```bash
# Ubuntu/Debian
sudo apt install libfuse-dev

# Fedora/RHEL
sudo dnf install fuse-devel

# macOS
brew install macfuse
```

### "Python.h not found"

Install Python development headers:
```bash
# Ubuntu/Debian
sudo apt install python3-dev

# Fedora/RHEL
sudo dnf install python3-devel
```

### "maturin not found"

Install maturin:
```bash
pip install maturin
```

### Build fails on macOS

Ensure Command Line Tools installed:
```bash
xcode-select --install
```

## Updating Hexz

```bash
cd hexz
git pull
make clean
make build
```

## Uninstalling

### Remove CLI
```bash
sudo rm /usr/local/bin/hexz
```

### Remove Python Package
```bash
pip uninstall hexz
```

### Remove Source
```bash
rm -rf ~/hexz
```

## Next Steps

- [Tutorial: Getting Started](../../tutorials/getting-started.md)
- [How-To: Pack Datasets](pack-datasets.md)
- [Reference: CLI Commands](../../reference/cli-reference.md)
