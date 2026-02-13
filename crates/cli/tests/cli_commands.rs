/// Integration tests for all CLI commands
///
/// This tests the full CLI stack end-to-end by invoking commands
/// and verifying their behavior.
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

mod common;
use common::TestEnv;

/// Helper to create a strata CLI command
fn strata() -> Command {
    #[allow(deprecated)]
    {
        Command::cargo_bin("strata").expect("Failed to find strata binary")
    }
}

#[test]
fn test_cli_help() {
    strata()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("strata"));
}

#[test]
fn test_cli_version() {
    strata()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("strata"));
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Data Commands
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_data_pack_basic() {
    let env = TestEnv::new();
    let input_file = env.create_test_file("test.bin", 1024 * 1024); // 1 MB

    strata()
        .arg("data")
        .arg("pack")
        .arg("--disk")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .assert()
        .success();

    assert!(env.snapshot_path.exists(), "Snapshot should be created");
    assert!(
        fs::metadata(&env.snapshot_path).unwrap().len() > 0,
        "Snapshot should not be empty"
    );
}

#[test]
fn test_data_pack_with_compression_lz4() {
    let env = TestEnv::new();
    let input_file = env.create_pattern_file("test.txt", b"Hello World! ", 10000);

    strata()
        .arg("data")
        .arg("pack")
        .arg("--disk")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .arg("--compression")
        .arg("lz4")
        .assert()
        .success();

    assert!(env.snapshot_path.exists());
}

#[test]
fn test_data_pack_with_compression_zstd() {
    let env = TestEnv::new();
    let input_file = env.create_pattern_file("test.txt", b"Compressible data ", 10000);

    strata()
        .arg("data")
        .arg("pack")
        .arg("--disk")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .arg("--compression")
        .arg("zstd")
        .assert()
        .success();

    assert!(env.snapshot_path.exists());
}

#[test]
fn test_data_pack_with_cdc() {
    let env = TestEnv::new();
    // Create file with repeating pattern (good for CDC)
    let input_file = env.create_pattern_file("dedup.bin", &[0xAB; 4096], 100);

    strata()
        .arg("data")
        .arg("pack")
        .arg("--disk")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .arg("--cdc")
        .assert()
        .success();

    assert!(env.snapshot_path.exists());
}

#[test]
fn test_data_pack_nonexistent_file() {
    let env = TestEnv::new();

    strata()
        .arg("data")
        .arg("pack")
        .arg("--disk")
        .arg("/nonexistent/file.bin")
        .arg("-o")
        .arg(&env.snapshot_path)
        .assert()
        .failure();
}

#[test]
fn test_data_info() {
    let env = TestEnv::new();
    let input_file = env.create_test_file("test.bin", 10240);

    // First create a snapshot
    strata()
        .arg("data")
        .arg("pack")
        .arg("--disk")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .assert()
        .success();

    // Then get info about it
    strata()
        .arg("data")
        .arg("info")
        .arg(&env.snapshot_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Archive").or(predicate::str::contains("Snapshot")));
}

#[test]
fn test_data_info_nonexistent() {
    strata()
        .arg("data")
        .arg("info")
        .arg("/nonexistent/snapshot.strata")
        .assert()
        .failure();
}

// ═══════════════════════════════════════════════════════════════════════════════
//  System Commands
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_sys_keygen() {
    let temp_dir = TempDir::new().unwrap();

    strata()
        .arg("sys")
        .arg("keygen")
        .arg("--output-dir")
        .arg(temp_dir.path())
        .assert()
        .success();

    // Check that keys were created in the directory
    let entries: Vec<_> = fs::read_dir(temp_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();

    assert!(!entries.is_empty(), "Keys should be created");
}

#[test]
fn test_sys_doctor() {
    strata().arg("sys").arg("doctor").assert().success();
}

// ═══════════════════════════════════════════════════════════════════════════════
//  E2E: Pack + Read workflow
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_e2e_disk_and_memory_pack() {
    let env = TestEnv::new();

    let disk_file = env.create_test_file("disk.img", 8192);
    let memory_file = env.create_test_file("memory.dump", 4096);

    // Pack both disk and memory
    strata()
        .arg("data")
        .arg("pack")
        .arg("--disk")
        .arg(&disk_file)
        .arg("--memory")
        .arg(&memory_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .assert()
        .success();

    // Verify snapshot exists
    assert!(env.snapshot_path.exists());
    let snapshot_size = fs::metadata(&env.snapshot_path).unwrap().len();
    assert!(snapshot_size > 0);

    // Get info about the snapshot
    strata()
        .arg("data")
        .arg("info")
        .arg(&env.snapshot_path)
        .assert()
        .success();
}

#[test]
fn test_e2e_compression_comparison() {
    let env = TestEnv::new();
    let input_file = env.create_pattern_file("compressible.txt", b"AAAABBBBCCCCDDDD", 1000);

    let lz4_snap = env.temp_dir.path().join("lz4.st");
    let zstd_snap = env.temp_dir.path().join("zstd.st");

    // Pack with LZ4
    strata()
        .arg("data")
        .arg("pack")
        .arg("--disk")
        .arg(&input_file)
        .arg("-o")
        .arg(&lz4_snap)
        .arg("--compression")
        .arg("lz4")
        .assert()
        .success();

    // Pack with Zstd
    strata()
        .arg("data")
        .arg("pack")
        .arg("--disk")
        .arg(&input_file)
        .arg("-o")
        .arg(&zstd_snap)
        .arg("--compression")
        .arg("zstd")
        .assert()
        .success();

    // Both should exist
    assert!(lz4_snap.exists());
    assert!(zstd_snap.exists());

    // Both should have reasonable sizes
    let lz4_size = fs::metadata(&lz4_snap).unwrap().len();
    let zstd_size = fs::metadata(&zstd_snap).unwrap().len();
    assert!(lz4_size > 0);
    assert!(zstd_size > 0);
}

#[test]
fn test_e2e_pack_info_roundtrip() {
    let env = TestEnv::new();
    let input_file = env.create_test_file("roundtrip.bin", 16384);

    // Pack
    strata()
        .arg("data")
        .arg("pack")
        .arg("--disk")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .assert()
        .success();

    // Info with JSON output
    strata()
        .arg("data")
        .arg("info")
        .arg(&env.snapshot_path)
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("{"));
}

#[test]
fn test_e2e_silent_mode() {
    let env = TestEnv::new();
    let input_file = env.create_test_file("silent.bin", 4096);

    // Pack with silent flag
    let output = strata()
        .arg("data")
        .arg("pack")
        .arg("--disk")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .arg("--silent")
        .assert()
        .success();

    // In silent mode, stdout should be minimal
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(
        stdout.len() < 100,
        "Silent mode should produce minimal output"
    );
}

#[test]
fn test_e2e_custom_block_size() {
    let env = TestEnv::new();
    let input_file = env.create_test_file("blocks.bin", 262144); // 256KB

    // Pack with custom block size (128KB)
    strata()
        .arg("data")
        .arg("pack")
        .arg("--disk")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .arg("--block-size")
        .arg("131072")
        .assert()
        .success();

    assert!(env.snapshot_path.exists());
}

#[test]
fn test_data_pack_with_train_dict() {
    let env = TestEnv::new();
    // Zstd dictionary training requires significant sample data
    // The zstd library recommends 100x the dictionary size (110KB * 100 = ~11MB)
    // Create a 1MB file to ensure we have enough training data
    let input_file = env.create_pattern_file(
        "dict_train.txt",
        b"Training data sample pattern for zstd dictionary learning! ",
        18000,
    );

    strata()
        .arg("data")
        .arg("pack")
        .arg("--disk")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .arg("--compression")
        .arg("zstd")
        .arg("--train-dict")
        .assert()
        .success();

    assert!(env.snapshot_path.exists());
}

// Encryption test is ignored because it requires interactive password input via /dev/tty,
// which is not available in non-interactive test environments (fails with ENXIO).
// TODO: Add support for STRATA_PASSWORD env var to enable non-interactive encryption
#[test]
#[ignore = "requires interactive password input"]
fn test_data_pack_with_encryption() {
    let env = TestEnv::new();
    let input_file = env.create_test_file("encrypted.bin", 8192);

    strata()
        .arg("data")
        .arg("pack")
        .arg("--disk")
        .arg(&input_file)
        .arg("-o")
        .arg(&env.snapshot_path)
        .arg("--encrypt")
        .assert()
        .success();

    assert!(env.snapshot_path.exists());
}
