# Test Coverage Improvement Plan

## Current Coverage: 14.74%

This document outlines the test strategy to bring Hexz to production-ready coverage (>60%).

## Coverage Targets by Module

### High Priority (Core Functionality)
- [ ] **CLI Commands** (currently 0%) → Target: 80%
  - [x] Basic integration tests created
  - [ ] Add tests for all subcommands
  - [ ] Error handling tests
  - [ ] Flag combination tests

- [ ] **Python Bindings** (currently 0%) → Target: 70%
  - [x] Basic integration tests created
  - [ ] Add PyTorch DataLoader tests
  - [ ] Add async dataset tests
  - [ ] Error propagation tests

- [ ] **Storage Backends** → Target: 70%
  - [x] Local (file/mmap): 95%+ ✅
  - [ ] HTTP: 0% → 60%
  - [ ] S3: 0% → 60%
  - Strategy: Use mock servers (wiremock/localstack)

- [ ] **Prefetch** (currently 0%) → Target: 60%
  - [x] Basic unit tests created
  - [ ] Integration with read path
  - [ ] Adaptive pattern detection

### Medium Priority
- [ ] **FUSE** (currently 0%) → Target: 50%
  - [ ] Mount/unmount tests
  - [ ] Read operations
  - [ ] Write overlay tests

- [ ] **Streaming Writer** (currently 0%) → Target: 60%
  - [ ] Basic write tests
  - [ ] Dedup integration
  - [ ] Metadata persistence

- [ ] **Format/Header** (currently 17%) → Target: 80%
  - [ ] Serialization round-trip tests
  - [ ] Version compatibility tests
  - [ ] Corruption detection

### Lower Priority (Advanced Features)
- [ ] **DCAM Dedup** (currently 5%) → Target: 40%
- [ ] **B-Tree Index** (currently 0%) → Target: 30%
- [ ] **NBD Server** (currently 0%) → Target: 30%

## Test Organization

```
tests/                          # Workspace integration tests
├── cli_commands.rs            # ✅ Created
├── python_loader.rs           # ✅ Created
├── cloud_backends.rs          # TODO
├── fuse_mount.rs              # TODO
└── pytorch_e2e.rs             # TODO

crates/*/tests/
├── unit/                      # Unit tests per module
│   ├── prefetch_tests.rs     # ✅ Created
│   ├── http_backend_tests.rs # ✅ Created (needs mock)
│   └── writer_tests.rs       # TODO
└── integration/               # Crate-level integration
    └── pack_read_tests.rs    # ✅ Exists
```

## Testing Strategy

### Phase 1: CLI & Python (Week 1)
1. ✅ Add `make test-cov` targets
2. ✅ Create CLI integration tests
3. ✅ Create Python loader tests
4. Run coverage, verify improvement
5. Fix failing tests

### Phase 2: Cloud Backends (Week 2)
1. Set up mock HTTP server (wiremock)
2. Set up localstack for S3 tests
3. Write backend integration tests
4. Test retry/error handling

### Phase 3: Advanced Features (Week 3)
1. FUSE mount tests (requires FUSE installed)
2. Streaming writer tests
3. Format version compatibility tests
4. Property-based tests (proptest)

### Phase 4: E2E & Performance (Week 4)
1. Full PyTorch training loop test
2. Concurrent reader stress tests
3. Large dataset tests (1GB+)
4. Performance regression baselines

## Quick Wins (Can be done now)

1. ✅ Add Makefile test-cov targets
2. ✅ Create test/ directory with integration tests
3. ✅ Add prefetch unit tests
4. ✅ Add HTTP backend skeleton tests
5. [ ] Add writer unit tests
6. [ ] Add format serialization tests
7. [ ] Add DCAM parameter tuning tests

## Dependencies Needed

```toml
[dev-dependencies]
assert_cmd = "2.0"       # CLI testing
predicates = "3.0"       # Assertion helpers
wiremock = "0.5"         # HTTP mock server
tempfile = "3.8"         # Temp directories
proptest = "1.4"         # Property-based testing
```

## Coverage Milestones

- **Week 1**: 15% → 35% (CLI + Python)
- **Week 2**: 35% → 50% (Cloud backends)
- **Week 3**: 50% → 60% (FUSE + Writer)
- **Week 4**: 60% → 65% (E2E polish)

## Next Steps

1. Run `make test-cov-rust` to verify new tests compile
2. Fix compilation errors in HTTP backend tests
3. Add writer unit tests
4. Set up mock servers for cloud backend tests
5. Write FUSE integration tests

## Notes

- Focus on **happy path + error handling** first
- Defer edge cases until core coverage is good
- Use `#[ignore]` for tests requiring external services
- Document any test environment requirements
