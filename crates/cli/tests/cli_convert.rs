/// Integration tests for `hexz data convert`
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

mod common;
use common::TestEnv;

/// Helper to create a hexz CLI command
fn hexz() -> Command {
    #[allow(deprecated)]
    {
        Command::cargo_bin("hexz").expect("Failed to find hexz binary")
    }
}

/// Create a simple tar archive with test files.
fn create_test_tar(
    dir: &std::path::Path,
    name: &str,
    files: &[(&str, &[u8])],
) -> std::path::PathBuf {
    let tar_path = dir.join(name);
    let file = fs::File::create(&tar_path).unwrap();
    let mut builder = tar::Builder::new(file);

    for (fname, data) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, fname, &data[..]).unwrap();
    }

    builder.finish().unwrap();
    tar_path
}

#[test]
fn test_data_convert_tar() {
    let env = TestEnv::new();
    let tar_path = create_test_tar(
        env.path(),
        "input.tar",
        &[("hello.txt", b"Hello, world!"), ("data.bin", &[0xAB; 4096])],
    );

    hexz()
        .arg("data")
        .arg("convert")
        .arg("tar")
        .arg(&tar_path)
        .arg(&env.snapshot_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Converted 2 files"));

    assert!(env.snapshot_path.exists(), "Snapshot should be created");
    assert!(
        fs::metadata(&env.snapshot_path).unwrap().len() > 0,
        "Snapshot should not be empty"
    );
}

#[test]
fn test_data_convert_tar_with_compression() {
    let env = TestEnv::new();
    let tar_path = create_test_tar(
        env.path(),
        "zstd_input.tar",
        &[("compressible.txt", &[0x41; 8192])],
    );

    hexz()
        .arg("data")
        .arg("convert")
        .arg("tar")
        .arg(&tar_path)
        .arg(&env.snapshot_path)
        .arg("--compression")
        .arg("zstd")
        .assert()
        .success();

    assert!(env.snapshot_path.exists());
}

#[test]
fn test_data_convert_tar_silent() {
    let env = TestEnv::new();
    let tar_path = create_test_tar(env.path(), "silent_input.tar", &[("file.txt", b"data")]);

    let output = hexz()
        .arg("data")
        .arg("convert")
        .arg("tar")
        .arg(&tar_path)
        .arg(&env.snapshot_path)
        .arg("--silent")
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(
        stdout.len() < 10,
        "Silent mode should produce minimal output"
    );
}

#[test]
fn test_data_convert_tar_info_roundtrip() {
    let env = TestEnv::new();
    let tar_path = create_test_tar(env.path(), "roundtrip.tar", &[("test.bin", &[0xCC; 2048])]);

    // Convert
    hexz()
        .arg("data")
        .arg("convert")
        .arg("tar")
        .arg(&tar_path)
        .arg(&env.snapshot_path)
        .assert()
        .success();

    // Inspect should work on the resulting snapshot
    hexz()
        .arg("data")
        .arg("info")
        .arg(&env.snapshot_path)
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("{"));
}

#[test]
fn test_data_convert_nonexistent() {
    let env = TestEnv::new();

    hexz()
        .arg("data")
        .arg("convert")
        .arg("tar")
        .arg("/nonexistent/file.tar")
        .arg(&env.snapshot_path)
        .assert()
        .failure();
}

#[test]
fn test_data_convert_unknown_format() {
    let env = TestEnv::new();
    let dummy = env.create_test_file("dummy.bin", 100);

    hexz()
        .arg("data")
        .arg("convert")
        .arg("parquet")
        .arg(&dummy)
        .arg(&env.snapshot_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unknown format"));
}

#[test]
fn test_data_convert_help() {
    hexz()
        .arg("data")
        .arg("convert")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Convert"));
}
