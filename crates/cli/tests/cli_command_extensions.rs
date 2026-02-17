/// Extended integration tests for CLI commands (analyze, build, diff, bench, sign, verify)
///
/// These tests complement cli_commands.rs with coverage for commands that were
/// previously untested according to TEST_PLAN.md Phase 1.1
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

mod common;
use common::TestEnv;

/// Helper to create a hexz CLI command
fn hexz() -> Command {
    #[allow(deprecated)]
    {
        Command::cargo_bin("hexz").expect("Failed to find hexz binary")
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Data Analyze Command Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_data_analyze_small_file() {
    let env = TestEnv::new();
    // File smaller than 512 MiB - should analyze entire file
    let input_file = env.create_pattern_file("small.bin", &[0xAB; 1024], 1000);

    hexz()
        .arg("analyze")
        .arg(&input_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("DCAM"));
}

#[test]
fn test_data_analyze_nonexistent_file() {
    hexz()
        .arg("analyze")
        .arg("/nonexistent/file.bin")
        .assert()
        .failure();
}

#[test]
fn test_data_analyze_compressible_data() {
    let env = TestEnv::new();
    // Highly compressible pattern - should yield good dedup ratio
    let input_file = env.create_pattern_file("compressible.bin", b"AAAAAAAAAA", 100000);

    hexz()
        .arg("analyze")
        .arg(&input_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("Predicted Ratio"));
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Data Build Command Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_data_build_generic_profile() {
    let env = TestEnv::new();
    let input_file = env.create_test_file("build_test.bin", 1024 * 1024);

    hexz()
        .arg("build")
        .arg("--source")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .arg("--profile")
        .arg("generic")
        .assert()
        .success();

    assert!(env.snapshot_path.exists());
}

#[test]
fn test_data_build_eda_profile() {
    let env = TestEnv::new();
    let input_file = env.create_test_file("eda.bin", 2 * 1024 * 1024);

    hexz()
        .arg("build")
        .arg("--source")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .arg("--profile")
        .arg("eda")
        .assert()
        .success();

    assert!(env.snapshot_path.exists());
}

#[test]
fn test_data_build_embedded_profile() {
    let env = TestEnv::new();
    let input_file = env.create_test_file("embedded.bin", 512 * 1024);

    hexz()
        .arg("build")
        .arg("--source")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .arg("--profile")
        .arg("embedded")
        .assert()
        .success();

    assert!(env.snapshot_path.exists());
}

#[test]
fn test_data_build_ml_profile() {
    let env = TestEnv::new();
    let input_file = env.create_test_file("ml.bin", 3 * 1024 * 1024);

    hexz()
        .arg("build")
        .arg("--source")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .arg("--profile")
        .arg("ml")
        .assert()
        .success();

    assert!(env.snapshot_path.exists());
}

#[test]
fn test_data_build_unknown_profile() {
    let env = TestEnv::new();
    let input_file = env.create_test_file("test.bin", 1024 * 1024);

    // Unknown profile should fallback to generic with warning
    hexz()
        .arg("build")
        .arg("--source")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .arg("--profile")
        .arg("unknown_profile")
        .assert()
        .success(); // Should still succeed with fallback

    assert!(env.snapshot_path.exists());
}

#[test]
fn test_data_build_with_cdc() {
    let env = TestEnv::new();
    let input_file = env.create_pattern_file("cdc_test.bin", &[0xAB; 4096], 100);

    hexz()
        .arg("build")
        .arg("--source")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .arg("--profile")
        .arg("generic")
        .arg("--cdc")
        .assert()
        .success();

    assert!(env.snapshot_path.exists());
}

#[test]
fn test_data_build_missing_source() {
    let env = TestEnv::new();

    hexz()
        .arg("build")
        .arg("--source")
        .arg("/nonexistent/source.bin")
        .arg("-o")
        .arg(&env.snapshot_path)
        .assert()
        .failure();
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Data Diff Command Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_data_diff_with_metadata() {
    let env = TestEnv::new();

    // Create a mock overlay file and metadata file
    let overlay_path = env.temp_dir.path().join("vm-state.overlay");
    let meta_path = env.temp_dir.path().join("vm-state.meta");

    // Create overlay file (sparse file)
    fs::write(&overlay_path, vec![0u8; 4096]).unwrap();

    // Create metadata file with 3 block indices (u64 little-endian)
    // Block indices: 0, 10, 100
    let mut metadata = Vec::new();
    metadata.extend_from_slice(&0u64.to_le_bytes());
    metadata.extend_from_slice(&10u64.to_le_bytes());
    metadata.extend_from_slice(&100u64.to_le_bytes());
    fs::write(&meta_path, metadata).unwrap();

    hexz()
        .arg("diff")
        .arg(&overlay_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("3")); // 3 modified blocks
}

#[test]
fn test_data_diff_with_blocks_flag() {
    let env = TestEnv::new();

    let overlay_path = env.temp_dir.path().join("test.overlay");
    let meta_path = env.temp_dir.path().join("test.meta");

    fs::write(&overlay_path, vec![0u8; 8192]).unwrap();

    // 5 blocks modified
    let mut metadata = Vec::new();
    for i in 0..5u64 {
        metadata.extend_from_slice(&i.to_le_bytes());
    }
    fs::write(&meta_path, metadata).unwrap();

    hexz()
        .arg("diff")
        .arg(&overlay_path)
        .arg("--blocks")
        .assert()
        .success()
        .stdout(predicate::str::contains("Overlay Statistics"));
}

#[test]
fn test_data_diff_no_metadata() {
    let env = TestEnv::new();

    let overlay_path = env.temp_dir.path().join("no-meta.overlay");
    fs::write(&overlay_path, vec![0u8; 4096]).unwrap();
    // No .meta file created

    hexz()
        .arg("diff")
        .arg(&overlay_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("No metadata file found"));
}

#[test]
fn test_data_diff_nonexistent_overlay() {
    hexz()
        .arg("diff")
        .arg("/nonexistent/overlay.overlay")
        .assert()
        .success()
        .stdout(predicate::str::contains("No metadata file found"));
}

#[test]
fn test_data_diff_with_files_flag() {
    let env = TestEnv::new();

    let overlay_path = env.temp_dir.path().join("files.overlay");
    let meta_path = env.temp_dir.path().join("files.meta");

    fs::write(&overlay_path, vec![0u8; 4096]).unwrap();

    // 3 block indices
    let mut metadata = Vec::new();
    metadata.extend_from_slice(&5u64.to_le_bytes());
    metadata.extend_from_slice(&42u64.to_le_bytes());
    metadata.extend_from_slice(&100u64.to_le_bytes());
    fs::write(&meta_path, metadata).unwrap();

    hexz()
        .arg("diff")
        .arg(&overlay_path)
        .arg("--files")
        .assert()
        .success()
        .stdout(predicate::str::contains("Modified Block Indices"))
        .stdout(predicate::str::contains("Block 5"))
        .stdout(predicate::str::contains("Block 42"))
        .stdout(predicate::str::contains("Block 100"));
}

#[test]
fn test_data_diff_blocks_and_files_flags() {
    let env = TestEnv::new();

    let overlay_path = env.temp_dir.path().join("both.overlay");
    let meta_path = env.temp_dir.path().join("both.meta");

    fs::write(&overlay_path, vec![0u8; 4096]).unwrap();

    let mut metadata = Vec::new();
    for i in 0..10u64 {
        metadata.extend_from_slice(&i.to_le_bytes());
    }
    fs::write(&meta_path, metadata).unwrap();

    hexz()
        .arg("diff")
        .arg(&overlay_path)
        .arg("--blocks")
        .arg("--files")
        .assert()
        .success()
        .stdout(predicate::str::contains("Overlay Statistics"))
        .stdout(predicate::str::contains("Modified Block Indices"))
        .stdout(predicate::str::contains("Block 0"))
        .stdout(predicate::str::contains("Block 9"));
}

#[test]
fn test_data_diff_empty_metadata() {
    let env = TestEnv::new();

    let overlay_path = env.temp_dir.path().join("empty.overlay");
    let meta_path = env.temp_dir.path().join("empty.meta");

    fs::write(&overlay_path, vec![0u8; 4096]).unwrap();
    fs::write(&meta_path, Vec::<u8>::new()).unwrap(); // Empty metadata

    hexz()
        .arg("diff")
        .arg(&overlay_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("0")); // 0 blocks
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Sys Bench Command Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_sys_bench_valid_snapshot() {
    let env = TestEnv::new();
    let input_file = env.create_test_file("bench.bin", 10 * 1024 * 1024); // 10 MB

    // First create a snapshot
    hexz()
        .arg("pack")
        .arg("--disk")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .assert()
        .success();

    // Now benchmark it
    hexz()
        .arg("bench")
        .arg(&env.snapshot_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("MB/s"));
}

#[test]
fn test_sys_bench_nonexistent_snapshot() {
    hexz()
        .arg("bench")
        .arg("/nonexistent/snapshot.hxz")
        .assert()
        .failure();
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Sys Sign and Verify Command Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_sys_sign_and_verify_roundtrip() {
    let env = TestEnv::new();
    let input_file = env.create_test_file("sign_test.bin", 1024 * 1024);

    // Create a snapshot
    hexz()
        .arg("pack")
        .arg("--disk")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .assert()
        .success();

    // Generate keys
    let key_dir = env.temp_dir.path().join("keys");
    fs::create_dir(&key_dir).unwrap();

    hexz()
        .arg("keygen")
        .arg("--output-dir")
        .arg(&key_dir)
        .assert()
        .success();

    // Find the generated keys
    let entries: Vec<_> = fs::read_dir(&key_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(entries.len() >= 2, "Should have generated key files");

    let private_key = key_dir.join("private.key");
    let public_key = key_dir.join("public.key");

    // Sign the snapshot
    hexz()
        .arg("sign")
        .arg("--key")
        .arg(&private_key)
        .arg(&env.snapshot_path)
        .assert()
        .success();

    // Verify the signature
    hexz()
        .arg("verify")
        .arg("--key")
        .arg(&public_key)
        .arg(&env.snapshot_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Signature Verified"));
}

#[test]
fn test_sys_verify_unsigned_snapshot() {
    let env = TestEnv::new();
    let input_file = env.create_test_file("unsigned.bin", 512 * 1024);

    // Create an unsigned snapshot
    hexz()
        .arg("pack")
        .arg("--disk")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .assert()
        .success();

    // Generate keys
    let key_dir = env.temp_dir.path().join("keys");
    fs::create_dir(&key_dir).unwrap();
    hexz()
        .arg("keygen")
        .arg("--output-dir")
        .arg(&key_dir)
        .assert()
        .success();

    let public_key = key_dir.join("public.key");

    // Try to verify unsigned snapshot - should fail
    hexz()
        .arg("verify")
        .arg("--key")
        .arg(&public_key)
        .arg(&env.snapshot_path)
        .assert()
        .failure();
}

#[test]
fn test_sys_sign_nonexistent_snapshot() {
    let temp_dir = TempDir::new().unwrap();
    let key_path = temp_dir.path().join("key.priv");
    fs::write(&key_path, vec![0u8; 32]).unwrap();

    hexz()
        .arg("sign")
        .arg("--key")
        .arg(&key_path)
        .arg("/nonexistent/snapshot.hxz")
        .assert()
        .failure();
}
