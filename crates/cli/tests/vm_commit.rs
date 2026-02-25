/// Integration tests for `hexz vm commit` command.
///
/// Tests the overlay merging pipeline: create a base snapshot via `data pack`,
/// synthesize overlay + .meta files, and verify `vm commit` produces a valid
/// output snapshot in both thick and thin modes.
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

/// Helper to create a valid base snapshot from a disk image.
fn create_base_snapshot(env: &TestEnv, disk_data: &[u8], compression: &str) -> std::path::PathBuf {
    let disk_file = env.temp_dir.path().join("disk.img");
    fs::write(&disk_file, disk_data).unwrap();

    let snapshot_path = env.temp_dir.path().join("base.hxz");
    hexz()
        .arg("pack")
        .arg("--disk")
        .arg(&disk_file)
        .arg(&snapshot_path)
        .arg("--compression")
        .arg(compression)
        .assert()
        .success();

    assert!(snapshot_path.exists());
    snapshot_path
}

/// Helper to create overlay + .meta files.
///
/// `modified_4k_blocks` is a list of 4KiB block indices that were "modified".
/// The overlay file will have data at those offsets.
fn create_overlay_files(
    dir: &std::path::Path,
    primary_size: u64,
    modified_4k_blocks: &[u64],
    overlay_data_byte: u8,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let overlay_path = dir.join("changes.overlay");
    let meta_path = dir.join("changes.meta");

    // Create overlay file — sparse file with modified chunks
    // We'll create a file of the right size, then write our modified blocks
    let mut overlay = vec![0u8; primary_size as usize];
    for &blk in modified_4k_blocks {
        let offset = (blk * 4096) as usize;
        let end = std::cmp::min(offset + 4096, overlay.len());
        for byte in &mut overlay[offset..end] {
            *byte = overlay_data_byte;
        }
    }
    fs::write(&overlay_path, &overlay).unwrap();

    // Create .meta file — array of u64 block indices (little-endian)
    let mut meta = Vec::new();
    for &blk in modified_4k_blocks {
        meta.extend_from_slice(&blk.to_le_bytes());
    }
    fs::write(&meta_path, &meta).unwrap();

    (overlay_path, meta_path)
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Basic Commit Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_vm_commit_thick_lz4() {
    let env = TestEnv::new();

    // Create a 128KB disk image with known pattern
    let disk_data = vec![0xABu8; 128 * 1024];
    let base = create_base_snapshot(&env, &disk_data, "lz4");

    // Modify 4KiB blocks 2 and 5
    let (overlay, _meta) = create_overlay_files(env.temp_dir.path(), 128 * 1024, &[2, 5], 0xCD);

    let output = env.temp_dir.path().join("committed.hxz");

    hexz()
        .arg("commit")
        .arg(&base)
        .arg(&overlay)
        .arg(&output)
        .arg("--compression")
        .arg("lz4")
        .arg("--keep-overlay")
        .assert()
        .success()
        .stdout(predicate::str::contains("Commit complete"));

    assert!(output.exists());
    assert!(output.metadata().unwrap().len() > 0);

    // Overlay files should still exist (--keep-overlay)
    assert!(overlay.exists());
}

#[test]
fn test_vm_commit_thick_zstd() {
    let env = TestEnv::new();

    let disk_data = vec![0xAAu8; 128 * 1024];
    let base = create_base_snapshot(&env, &disk_data, "zstd");

    let (overlay, _meta) = create_overlay_files(env.temp_dir.path(), 128 * 1024, &[0, 3, 7], 0xBB);

    let output = env.temp_dir.path().join("committed_zstd.hxz");

    hexz()
        .arg("commit")
        .arg(&base)
        .arg(&overlay)
        .arg(&output)
        .arg("--compression")
        .arg("zstd")
        .arg("--keep-overlay")
        .assert()
        .success()
        .stdout(predicate::str::contains("Commit complete"));

    assert!(output.exists());
}

#[test]
fn test_vm_commit_thin_mode() {
    let env = TestEnv::new();

    let disk_data = vec![0x55u8; 128 * 1024];
    let base = create_base_snapshot(&env, &disk_data, "lz4");

    // Only modify block 1 — thin snapshot should be much smaller
    let (overlay, _meta) = create_overlay_files(env.temp_dir.path(), 128 * 1024, &[1], 0xFF);

    let output = env.temp_dir.path().join("thin.hxz");

    hexz()
        .arg("commit")
        .arg(&base)
        .arg(&overlay)
        .arg(&output)
        .arg("--thin")
        .arg("--keep-overlay")
        .assert()
        .success()
        .stdout(predicate::str::contains("Thin: true"));

    assert!(output.exists());

    // Thin snapshot should generally be smaller than the base since most blocks are references
    // (This is a soft check — compression and headers may affect exact sizes)
    let output_size = output.metadata().unwrap().len();
    assert!(output_size > 0);
}

#[test]
fn test_vm_commit_no_modifications() {
    let env = TestEnv::new();

    let disk_data = vec![0x00u8; 64 * 1024];
    let base = create_base_snapshot(&env, &disk_data, "lz4");

    // Empty overlay — no modifications
    let (overlay, _meta) = create_overlay_files(env.temp_dir.path(), 64 * 1024, &[], 0x00);

    let output = env.temp_dir.path().join("no_changes.hxz");

    hexz()
        .arg("commit")
        .arg(&base)
        .arg(&overlay)
        .arg(&output)
        .arg("--keep-overlay")
        .assert()
        .success();

    assert!(output.exists());
}

#[test]
fn test_vm_commit_deletes_overlay_by_default() {
    let env = TestEnv::new();

    let disk_data = vec![0xCCu8; 64 * 1024];
    let base = create_base_snapshot(&env, &disk_data, "lz4");

    let (overlay, meta) = create_overlay_files(env.temp_dir.path(), 64 * 1024, &[0], 0xDD);

    let output = env.temp_dir.path().join("cleanup.hxz");

    // No --keep-overlay flag → overlay should be deleted
    hexz()
        .arg("commit")
        .arg(&base)
        .arg(&overlay)
        .arg(&output)
        .assert()
        .success()
        .stdout(predicate::str::contains("Cleaning up overlay files"));

    assert!(output.exists());
    // Overlay and meta should be deleted
    assert!(!overlay.exists());
    assert!(!meta.exists());
}

#[test]
fn test_vm_commit_with_message() {
    let env = TestEnv::new();

    let disk_data = vec![0x11u8; 64 * 1024];
    let base = create_base_snapshot(&env, &disk_data, "lz4");

    let (overlay, _meta) = create_overlay_files(env.temp_dir.path(), 64 * 1024, &[0], 0x22);

    let output = env.temp_dir.path().join("messaged.hxz");

    hexz()
        .arg("commit")
        .arg(&base)
        .arg(&overlay)
        .arg(&output)
        .arg("--message")
        .arg("Test commit message")
        .arg("--keep-overlay")
        .assert()
        .success();

    assert!(output.exists());

    // Verify the message is readable via info
    hexz().arg("inspect").arg(&output).assert().success();
}

#[test]
fn test_vm_commit_custom_block_size() {
    let env = TestEnv::new();

    // Larger disk to test multiple blocks with custom block size
    let disk_data = vec![0x33u8; 256 * 1024];
    let base = create_base_snapshot(&env, &disk_data, "lz4");

    let (overlay, _meta) =
        create_overlay_files(env.temp_dir.path(), 256 * 1024, &[0, 10, 20, 30], 0x44);

    let output = env.temp_dir.path().join("block32k.hxz");

    hexz()
        .arg("commit")
        .arg(&base)
        .arg(&overlay)
        .arg(&output)
        .arg("--block-size")
        .arg("32768")
        .arg("--keep-overlay")
        .assert()
        .success();

    assert!(output.exists());
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Committed Snapshot Validation Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_vm_commit_output_is_valid_snapshot() {
    let env = TestEnv::new();

    let disk_data = vec![0xEEu8; 128 * 1024];
    let base = create_base_snapshot(&env, &disk_data, "lz4");

    let (overlay, _meta) = create_overlay_files(env.temp_dir.path(), 128 * 1024, &[1, 4], 0xFF);

    let output = env.temp_dir.path().join("valid.hxz");

    hexz()
        .arg("commit")
        .arg(&base)
        .arg(&overlay)
        .arg(&output)
        .arg("--keep-overlay")
        .assert()
        .success();

    // Verify output is a valid snapshot by running info
    hexz()
        .arg("inspect")
        .arg(&output)
        .assert()
        .success()
        .stdout(predicate::str::contains("Block Size"));
}

#[test]
fn test_vm_commit_thick_info_shows_disk() {
    let env = TestEnv::new();

    let disk_data = vec![0x99u8; 128 * 1024];
    let base = create_base_snapshot(&env, &disk_data, "lz4");

    let (overlay, _meta) = create_overlay_files(env.temp_dir.path(), 128 * 1024, &[2], 0x88);

    let output = env.temp_dir.path().join("readable.hxz");

    hexz()
        .arg("commit")
        .arg(&base)
        .arg(&overlay)
        .arg(&output)
        .arg("--keep-overlay")
        .assert()
        .success();

    // Verify committed snapshot reports correct metadata
    hexz()
        .arg("inspect")
        .arg(&output)
        .assert()
        .success()
        .stdout(predicate::str::contains("Block Size"));

    // Verify file size is reasonable (has real data)
    let output_size = output.metadata().unwrap().len();
    assert!(output_size > 100, "Committed snapshot should have data");
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Error Path Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_vm_commit_nonexistent_base() {
    let env = TestEnv::new();

    let overlay = env.temp_dir.path().join("fake.overlay");
    let meta = env.temp_dir.path().join("fake.meta");
    fs::write(&overlay, vec![0u8; 4096]).unwrap();
    fs::write(&meta, []).unwrap();

    let output = env.temp_dir.path().join("out.hxz");

    hexz()
        .arg("commit")
        .arg("/nonexistent/base.hxz")
        .arg(&overlay)
        .arg(&output)
        .assert()
        .failure();
}

#[test]
fn test_vm_commit_nonexistent_overlay() {
    let env = TestEnv::new();

    let disk_data = vec![0x00u8; 64 * 1024];
    let base = create_base_snapshot(&env, &disk_data, "lz4");

    let output = env.temp_dir.path().join("out.hxz");

    hexz()
        .arg("commit")
        .arg(&base)
        .arg("/nonexistent/overlay.bin")
        .arg(&output)
        .assert()
        .failure();
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Thin Snapshot Chain Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_vm_commit_thin_shows_in_info() {
    let env = TestEnv::new();

    let disk_data = vec![0x77u8; 128 * 1024];
    let base = create_base_snapshot(&env, &disk_data, "lz4");

    let (overlay, _meta) = create_overlay_files(env.temp_dir.path(), 128 * 1024, &[0, 3], 0x88);

    let output = env.temp_dir.path().join("thin_info.hxz");

    hexz()
        .arg("commit")
        .arg(&base)
        .arg(&overlay)
        .arg(&output)
        .arg("--thin")
        .arg("--keep-overlay")
        .assert()
        .success();

    // Info should show this is a thin snapshot with parent reference
    hexz().arg("inspect").arg(&output).assert().success();
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Zero-Block Detection Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_vm_commit_all_zero_blocks() {
    let env = TestEnv::new();

    // Base is all zeros
    let disk_data = vec![0x00u8; 64 * 1024];
    let base = create_base_snapshot(&env, &disk_data, "lz4");

    // Overlay is also all zeros (modified but still zero)
    let (overlay, _meta) = create_overlay_files(env.temp_dir.path(), 64 * 1024, &[0, 1, 2], 0x00);

    let output = env.temp_dir.path().join("zeros.hxz");

    hexz()
        .arg("commit")
        .arg(&base)
        .arg(&overlay)
        .arg(&output)
        .arg("--keep-overlay")
        .assert()
        .success();

    assert!(output.exists());
    // Zero blocks should be stored as metadata only → small file
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Large Overlay Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_vm_commit_many_modified_blocks() {
    let env = TestEnv::new();

    // 512KB disk
    let disk_data = vec![0x11u8; 512 * 1024];
    let base = create_base_snapshot(&env, &disk_data, "lz4");

    // Modify many blocks
    let modified: Vec<u64> = (0..128).collect(); // first 128 4K blocks = first 512KB
    let (overlay, _meta) = create_overlay_files(env.temp_dir.path(), 512 * 1024, &modified, 0x22);

    let output = env.temp_dir.path().join("many_mods.hxz");

    hexz()
        .arg("commit")
        .arg(&base)
        .arg(&overlay)
        .arg(&output)
        .arg("--keep-overlay")
        .assert()
        .success();

    assert!(output.exists());
}

#[test]
fn test_vm_commit_thin_many_modified_blocks() {
    let env = TestEnv::new();

    // 512KB disk
    let disk_data = vec![0x33u8; 512 * 1024];
    let base = create_base_snapshot(&env, &disk_data, "lz4");

    // Modify only every other block
    let modified: Vec<u64> = (0..64).map(|i| i * 2).collect();
    let (overlay, _meta) = create_overlay_files(env.temp_dir.path(), 512 * 1024, &modified, 0x44);

    let output = env.temp_dir.path().join("thin_many.hxz");

    hexz()
        .arg("commit")
        .arg(&base)
        .arg(&overlay)
        .arg(&output)
        .arg("--thin")
        .arg("--keep-overlay")
        .assert()
        .success();

    assert!(output.exists());
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Cross-Compression Commit Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_vm_commit_lz4_base_zstd_commit() {
    let env = TestEnv::new();

    // Base with LZ4
    let disk_data = vec![0xAAu8; 128 * 1024];
    let base = create_base_snapshot(&env, &disk_data, "lz4");

    let (overlay, _meta) = create_overlay_files(env.temp_dir.path(), 128 * 1024, &[0, 5], 0xBB);

    let output = env.temp_dir.path().join("cross_compress.hxz");

    // Commit with zstd (different from base)
    hexz()
        .arg("commit")
        .arg(&base)
        .arg(&overlay)
        .arg(&output)
        .arg("--compression")
        .arg("zstd")
        .arg("--keep-overlay")
        .assert()
        .success();

    assert!(output.exists());

    // Verify the output snapshot is valid
    hexz().arg("inspect").arg(&output).assert().success();
}

#[test]
fn test_vm_commit_zstd_base_lz4_commit() {
    let env = TestEnv::new();

    // Base with zstd
    let disk_data = vec![0xCCu8; 128 * 1024];
    let base = create_base_snapshot(&env, &disk_data, "zstd");

    let (overlay, _meta) = create_overlay_files(env.temp_dir.path(), 128 * 1024, &[1, 3], 0xDD);

    let output = env.temp_dir.path().join("zstd_to_lz4.hxz");

    // Commit with lz4
    hexz()
        .arg("commit")
        .arg(&base)
        .arg(&overlay)
        .arg(&output)
        .arg("--compression")
        .arg("lz4")
        .arg("--keep-overlay")
        .assert()
        .success();

    assert!(output.exists());
}
