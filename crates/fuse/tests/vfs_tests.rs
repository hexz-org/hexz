//! Unit tests for VFS layer: make_attr, InodeType, and Overlay.

use fuser::FileType;
use hexz_fuse::vfs::make_attr;

// ─── make_attr tests ───────────────────────────────────────────────────────

#[test]
fn test_make_attr_root_dir() {
    let attr = make_attr(1, 0, 1000, 1000);
    assert_eq!(attr.ino, 1);
    assert_eq!(attr.kind, FileType::Directory);
    assert_eq!(attr.perm, 0o755);
    assert_eq!(attr.nlink, 2);
    assert_eq!(attr.size, 0);
    assert_eq!(attr.uid, 1000);
    assert_eq!(attr.gid, 1000);
}

#[test]
fn test_make_attr_regular_file() {
    let attr = make_attr(2, 1024, 500, 500);
    assert_eq!(attr.ino, 2);
    assert_eq!(attr.kind, FileType::RegularFile);
    assert_eq!(attr.perm, 0o644);
    assert_eq!(attr.nlink, 1);
    assert_eq!(attr.size, 1024);
    assert_eq!(attr.uid, 500);
    assert_eq!(attr.gid, 500);
}

#[test]
fn test_make_attr_large_file() {
    let ten_gib = 10 * 1024 * 1024 * 1024u64;
    let attr = make_attr(2, ten_gib, 0, 0);
    assert_eq!(attr.size, ten_gib);
    // blocks = ceil(10GiB / 512) = 20971520
    assert_eq!(attr.blocks, ten_gib.div_ceil(512));
}

#[test]
fn test_make_attr_zero_size() {
    let attr = make_attr(3, 0, 1000, 1000);
    assert_eq!(attr.blocks, 0);
    assert_eq!(attr.size, 0);
}

#[test]
fn test_make_attr_uid_gid_propagation() {
    let attr = make_attr(2, 100, 42, 99);
    assert_eq!(attr.uid, 42);
    assert_eq!(attr.gid, 99);
}

#[test]
fn test_make_attr_block_count() {
    // 1 byte -> 1 block (512)
    let attr = make_attr(2, 1, 0, 0);
    assert_eq!(attr.blocks, 1);

    // 512 bytes -> 1 block
    let attr = make_attr(2, 512, 0, 0);
    assert_eq!(attr.blocks, 1);

    // 513 bytes -> 2 blocks
    let attr = make_attr(2, 513, 0, 0);
    assert_eq!(attr.blocks, 2);
}
