/// Extended integration tests for CLI commands (build, sign, verify)
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
//  Data Build Command Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_data_build_generic_profile() {
    let env = TestEnv::new();
    let input_file = env.create_test_file("build_test.bin", 1024 * 1024);

    hexz()
        .arg("build")
        .arg(&input_file)
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
        .arg(&input_file)
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
        .arg(&input_file)
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
        .arg(&input_file)
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
        .arg(&input_file)
        .arg(&env.snapshot_path)
        .arg("--profile")
        .arg("unknown_profile")
        .assert()
        .success(); // Should still succeed with fallback

    assert!(env.snapshot_path.exists());
}

#[test]
fn test_data_build_with_profile() {
    let env = TestEnv::new();
    let input_file = env.create_pattern_file("cdc_test.bin", &[0xAB; 4096], 100);

    hexz()
        .arg("build")
        .arg(&input_file)
        .arg(&env.snapshot_path)
        .arg("--profile")
        .arg("generic")
        .assert()
        .success();

    assert!(env.snapshot_path.exists());
}

#[test]
fn test_data_build_missing_source() {
    let env = TestEnv::new();

    hexz()
        .arg("build")
        .arg("/nonexistent/source.bin")
        .arg(&env.snapshot_path)
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
        .arg(&private_key)
        .arg(&env.snapshot_path)
        .assert()
        .success();

    // Verify the signature
    hexz()
        .arg("verify")
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
        .arg(&key_path)
        .arg("/nonexistent/snapshot.hxz")
        .assert()
        .failure();
}
