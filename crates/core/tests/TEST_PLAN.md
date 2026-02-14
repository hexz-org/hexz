# Hexz Core - Comprehensive Test Plan

## Test Organization

```
tests/
├── unit/              # Unit tests for individual modules
│   ├── format_tests.rs      # Format structures (COMPLETED)
│   ├── compression_tests.rs # Compression algorithms
│   ├── encryption_tests.rs  # Encryption/decryption
│   ├── store_tests.rs       # Storage backends
│   ├── cache_tests.rs       # Caching layers
│   └── algo_tests.rs        # Dedup algorithms
├── integration/       # Multi-module integration tests
│   ├── pack_tests.rs        # End-to-end packing
│   ├── read_tests.rs        # Reading snapshots
│   ├── thin_snapshot_tests.rs
│   └── concurrent_tests.rs  # Thread safety
├── property/          # Property-based tests (proptest)
│   ├── format_properties.rs
│   ├── compression_properties.rs
│   └── index_properties.rs
└── fixtures/          # Test data and utilities
    ├── mod.rs
    ├── generators.rs  # Data generation
    └── samples/       # Binary test files
