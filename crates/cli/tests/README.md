# Strata Integration Test Suite

This directory contains workspace-level integration tests that verify the entire Strata stack end-to-end.

## Test Organization

```
tests/
├── integration/           # Full stack E2E tests
│   ├── cli_commands.rs   # Test all CLI commands
│   ├── python_loader.rs  # Python bindings via PyO3
│   ├── cloud_backends.rs # S3/HTTP backend tests
│   ├── fuse_mount.rs     # FUSE filesystem tests
│   └── pytorch_e2e.rs    # Real PyTorch training loops
├── stress/               # Performance and stress tests
│   ├── concurrent.rs     # Concurrent access patterns
│   └── large_scale.rs    # 1TB+ snapshots, 100M+ samples
├── fixtures/             # Shared test data and utilities
│   ├── mod.rs
│   ├── datasets.rs       # Generate test datasets
│   └── tempenv.rs        # Temporary environment setup
└── common/               # Common test utilities
    └── mod.rs
```

## Coverage Goals

- **CLI**: >80% coverage on all commands
- **Python bindings**: >70% coverage on PyO3 interface
- **Cloud backends**: >60% coverage with mocked/local S3
- **FUSE**: >50% coverage on core operations
- **Overall**: >60% line coverage across the workspace

## Running Tests

```bash
# Run all integration tests
cargo test --test '*'

# Run specific test suite
cargo test --test cli_commands
cargo test --test python_loader

# Run with coverage
cargo llvm-cov --test '*' --html

# Python integration tests (separate)
cd crates/loader && pytest tests/
```

## Test Data

Tests use temporary directories and auto-cleanup. Some tests require:
- Python 3.8+ with PyTorch (for ML tests)
- Docker (for S3 mock server)
- FUSE support (for mount tests)
