//! Unit tests for VFS layer: make_attr, InodeType, and Overlay.

use fuser::FileType;
use hexz_fuse::vfs::{InodeType, Overlay, make_attr};
use tempfile::TempDir;

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

// ─── InodeType tests ───────────────────────────────────────────────────────

#[test]
fn test_inode_type_from_u64_root() {
    assert_eq!(InodeType::from_u64(1), Some(InodeType::Root));
}

#[test]
fn test_inode_type_from_u64_disk() {
    assert_eq!(InodeType::from_u64(2), Some(InodeType::Disk));
}

#[test]
fn test_inode_type_from_u64_memory() {
    assert_eq!(InodeType::from_u64(3), Some(InodeType::Memory));
}

#[test]
fn test_inode_type_from_u64_zero() {
    assert_eq!(InodeType::from_u64(0), None);
}

#[test]
fn test_inode_type_from_u64_four() {
    assert_eq!(InodeType::from_u64(4), None);
}

#[test]
fn test_inode_type_from_u64_max() {
    assert_eq!(InodeType::from_u64(u64::MAX), None);
}

#[test]
fn test_inode_type_as_u64() {
    assert_eq!(InodeType::Root as u64, 1);
    assert_eq!(InodeType::Disk as u64, 2);
    assert_eq!(InodeType::Memory as u64, 3);
}

// ─── Overlay tests ─────────────────────────────────────────────────────────

#[test]
fn test_overlay_new_empty() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("overlay.bin");
    let overlay = Overlay::new(&path).unwrap();
    assert!(overlay.modified_blocks.is_empty());
    assert!(overlay.is_empty());
}

#[test]
fn test_overlay_write_and_read() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("overlay.bin");
    let mut overlay = Overlay::new(&path).unwrap();

    let data = b"hello overlay";
    let written = overlay.write_file(0, data).unwrap();
    assert_eq!(written, data.len());

    let mut buf = vec![0u8; data.len()];
    let read = overlay.read_file(0, &mut buf).unwrap();
    assert_eq!(read, data.len());
    assert_eq!(&buf, data);
}

#[test]
fn test_overlay_mark_block_modified() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("overlay.bin");
    let mut overlay = Overlay::new(&path).unwrap();

    assert!(!overlay.is_block_modified(0));
    overlay.mark_block_modified(0).unwrap();
    assert!(overlay.is_block_modified(0));
    assert!(!overlay.is_block_modified(1));
}

#[test]
fn test_overlay_persistence() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("overlay.bin");

    // Write some data and mark blocks
    {
        let mut overlay = Overlay::new(&path).unwrap();
        let data = vec![0xAA; 8192]; // 2 blocks worth
        overlay.write_file(0, &data).unwrap();
        overlay.mark_block_modified(0).unwrap();
        overlay.mark_block_modified(1).unwrap();
        overlay.mark_block_modified(5).unwrap();
    }

    // Reopen and verify modified_blocks restored from .meta
    {
        let overlay = Overlay::new(&path).unwrap();
        assert!(overlay.is_block_modified(0));
        assert!(overlay.is_block_modified(1));
        assert!(overlay.is_block_modified(5));
        assert!(!overlay.is_block_modified(2));
    }
}

#[test]
fn test_overlay_len_and_is_empty() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("overlay.bin");
    let mut overlay = Overlay::new(&path).unwrap();

    assert_eq!(overlay.len(), 0);
    assert!(overlay.is_empty());

    let data = vec![0x42; 1024];
    overlay.write_file(0, &data).unwrap();

    assert_eq!(overlay.len(), 1024);
    assert!(!overlay.is_empty());
}

#[test]
fn test_overlay_dedup_marking() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("overlay.bin");
    let mut overlay = Overlay::new(&path).unwrap();

    overlay.mark_block_modified(7).unwrap();
    overlay.mark_block_modified(7).unwrap(); // duplicate
    overlay.mark_block_modified(7).unwrap(); // triplicate

    assert!(overlay.is_block_modified(7));

    // Reopen and verify only one entry for block 7
    drop(overlay);
    let overlay2 = Overlay::new(&path).unwrap();
    assert!(overlay2.is_block_modified(7));
    assert!(!overlay2.is_block_modified(6));
}
