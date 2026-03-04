//! FUSE filesystem implementation for Hexz archives.

mod read;

use crate::vfs::InodeMap;
use fuser::Filesystem;
use hexz_core::Archive;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub(crate) const TTL: Duration = Duration::from_secs(1);

pub struct Hexz {
    pub(crate) snap: Arc<Archive>,
    pub(crate) inodes: InodeMap,
    pub(crate) write_layer: Option<PathBuf>,
}

impl Hexz {
    pub fn new(
        snap: Arc<Archive>,
        uid: u32,
        gid: u32,
        write_layer: Option<PathBuf>,
        metadata_dir: Option<PathBuf>,
    ) -> anyhow::Result<Self> {
        let mut inodes = InodeMap::new(&snap, uid, gid);
        if let Some(ref base) = write_layer {
            inodes.populate_from_overlay(base);
        }
        if let Some(ref m_dir) = metadata_dir {
            inodes.populate_from_metadata_dir(m_dir);
        }

        Ok(Self {
            snap: snap.clone(),
            inodes,
            write_layer,
        })
    }
    fn overlay_path(&self, ino: u64) -> Option<PathBuf> {
        let base = self.write_layer.as_ref()?;
        let rel = self.inodes.get_path(ino)?;
        Some(base.join(rel))
    }

    fn ensure_cow(&self, ino: u64) -> std::io::Result<()> {
        let overlay = match self.overlay_path(ino) {
            Some(p) => p,
            None => return Ok(()),
        };

        if overlay.exists() {
            return Ok(());
        }

        // Copy from archive
        if let Some((stream, offset, size)) = self.inodes.file_info(ino) {
            if let Some(parent) = overlay.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let data = self
                .snap
                .read_at(stream, offset, size as usize)
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            std::fs::write(&overlay, data)?;
        }
        Ok(())
    }

    /// Internal getattr that doesn't send a FUSE reply, for internal lookups.
    fn getattr_internal(&self, ino: u64) -> fuser::FileAttr {
        // Check if file exists in passthrough paths
        if let Some(host_path) = self.inodes.passthrough_paths.get(&ino) {
            if let Ok(meta) = std::fs::metadata(host_path) {
                use std::os::unix::fs::MetadataExt;
                let mut attr = fuser::FileAttr {
                    ino,
                    size: meta.len(),
                    blocks: meta.blocks(),
                    atime: std::time::UNIX_EPOCH + Duration::from_secs(meta.atime() as u64),
                    mtime: std::time::UNIX_EPOCH + Duration::from_secs(meta.mtime() as u64),
                    ctime: std::time::SystemTime::UNIX_EPOCH
                        + Duration::from_secs(meta.ctime() as u64),
                    crtime: std::time::UNIX_EPOCH,
                    kind: if meta.is_dir() {
                        fuser::FileType::Directory
                    } else {
                        fuser::FileType::RegularFile
                    },
                    perm: (meta.mode() as u16) & 0o777,
                    nlink: meta.nlink() as u32,
                    uid: self.inodes.uid(), // Use mount UID
                    gid: self.inodes.gid(), // Use mount GID
                    rdev: meta.rdev() as u32,
                    blksize: meta.blksize() as u32,
                    flags: 0,
                };
                if self.write_layer.is_some() {
                    attr.perm |= 0o222;
                }
                return attr;
            }
        }

        // Check if file exists in overlay
        if let (Some(base), Some(rel_path)) = (&self.write_layer, self.inodes.get_path(ino)) {
            let full_path = base.join(rel_path);
            if let Ok(meta) = std::fs::metadata(&full_path) {
                use std::os::unix::fs::MetadataExt;
                let mut attr = fuser::FileAttr {
                    ino,
                    size: meta.len(),
                    blocks: meta.blocks(),
                    atime: std::time::UNIX_EPOCH + Duration::from_secs(meta.atime() as u64),
                    mtime: std::time::UNIX_EPOCH + Duration::from_secs(meta.mtime() as u64),
                    ctime: std::time::SystemTime::UNIX_EPOCH
                        + Duration::from_secs(meta.ctime() as u64),
                    crtime: std::time::UNIX_EPOCH,
                    kind: if meta.is_dir() {
                        fuser::FileType::Directory
                    } else {
                        fuser::FileType::RegularFile
                    },
                    perm: (meta.mode() as u16) & 0o777,
                    nlink: meta.nlink() as u32,
                    uid: self.inodes.uid(), // Use mount UID
                    gid: self.inodes.gid(), // Use mount GID
                    rdev: meta.rdev() as u32,
                    blksize: meta.blksize() as u32,
                    flags: 0,
                };
                if self.write_layer.is_some() {
                    attr.perm |= 0o222;
                }
                return attr;
            }
        }

        let mut attr = self.inodes.getattr(ino);
        if self.write_layer.is_some() {
            attr.perm |= 0o222; // Add write bit
        }
        attr
    }
}

impl Filesystem for Hexz {
    fn access(&mut self, _req: &fuser::Request, _ino: u64, _mask: i32, reply: fuser::ReplyEmpty) {
        reply.ok();
    }

    fn lookup(
        &mut self,
        _req: &fuser::Request,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEntry,
    ) {
        if let Some(parent_path) = self.inodes.get_path(parent) {
            let rel_path = parent_path.join(name);
            if let Some(inode) = self.inodes.get_inode(&rel_path) {
                let attr = self.getattr_internal(inode);
                reply.entry(&TTL, &attr, 0);
                return;
            }
        }

        reply.error(libc::ENOENT);
    }

    fn open(&mut self, _req: &fuser::Request, _ino: u64, _flags: i32, reply: fuser::ReplyOpen) {
        reply.opened(1, 0);
    }

    fn opendir(&mut self, _req: &fuser::Request, _ino: u64, _flags: i32, reply: fuser::ReplyOpen) {
        reply.opened(1, 0);
    }

    fn create(
        &mut self,
        _req: &fuser::Request,
        parent: u64,
        name: &std::ffi::OsStr,
        _mode: u32,
        _umask: u32,
        _flags: i32,
        reply: fuser::ReplyCreate,
    ) {
        if let Some(base) = &self.write_layer {
            if let Some(parent_path) = self.inodes.get_path(parent) {
                let rel_path = parent_path.join(name);
                let full_path = base.join(&rel_path);
                if let Some(p) = full_path.parent() {
                    let _ = std::fs::create_dir_all(p);
                }

                match std::fs::File::create(&full_path) {
                    Ok(_) => {
                        let ino = self.inodes.add_file_at_path(&rel_path, false);
                        let attr = self.getattr_internal(ino);
                        reply.created(&TTL, &attr, 0, 1, 0);
                        return;
                    }
                    Err(e) => {
                        reply.error(e.raw_os_error().unwrap_or(libc::EIO));
                        return;
                    }
                }
            }
        }
        reply.error(libc::EROFS);
    }

    fn mknod(
        &mut self,
        _req: &fuser::Request,
        parent: u64,
        name: &std::ffi::OsStr,
        _mode: u32,
        _umask: u32,
        _rdev: u32,
        reply: fuser::ReplyEntry,
    ) {
        if let Some(base) = &self.write_layer {
            if let Some(parent_path) = self.inodes.get_path(parent) {
                let rel_path = parent_path.join(name);
                let full_path = base.join(&rel_path);
                if let Some(p) = full_path.parent() {
                    let _ = std::fs::create_dir_all(p);
                }

                match std::fs::File::create(&full_path) {
                    Ok(_) => {
                        let ino = self.inodes.add_file_at_path(&rel_path, false);
                        let attr = self.getattr_internal(ino);
                        reply.entry(&TTL, &attr, 0);
                        return;
                    }
                    Err(e) => {
                        reply.error(e.raw_os_error().unwrap_or(libc::EIO));
                        return;
                    }
                }
            }
        }
        reply.error(libc::EROFS);
    }

    fn mkdir(
        &mut self,
        _req: &fuser::Request,
        parent: u64,
        name: &std::ffi::OsStr,
        _mode: u32,
        _umask: u32,
        reply: fuser::ReplyEntry,
    ) {
        if let Some(base) = &self.write_layer {
            if let Some(parent_path) = self.inodes.get_path(parent) {
                let rel_path = parent_path.join(name);
                let full_path = base.join(&rel_path);
                match std::fs::create_dir_all(&full_path) {
                    Ok(_) => {
                        let ino = self.inodes.add_file_at_path(&rel_path, true);
                        let attr = self.inodes.getattr(ino);
                        reply.entry(&TTL, &attr, 0);
                        return;
                    }
                    Err(e) => {
                        reply.error(e.raw_os_error().unwrap_or(libc::EIO));
                        return;
                    }
                }
            }
        }
        reply.error(libc::EROFS);
    }

    fn getattr(&mut self, _req: &fuser::Request, ino: u64, reply: fuser::ReplyAttr) {
        if !self.inodes.is_valid_inode(ino) {
            reply.error(libc::ENOENT);
            return;
        }
        let attr = self.getattr_internal(ino);
        reply.attr(&TTL, &attr);
    }

    fn statfs(&mut self, _req: &fuser::Request, _ino: u64, reply: fuser::ReplyStatfs) {
        if let Some(ref base) = self.write_layer {
            use std::ffi::CString;
            use std::os::unix::ffi::OsStrExt;

            let path = CString::new(base.as_os_str().as_bytes()).unwrap();
            unsafe {
                let mut stats = std::mem::zeroed();
                if libc::statvfs(path.as_ptr(), &mut stats) == 0 {
                    reply.statfs(
                        stats.f_blocks as u64,
                        stats.f_bfree as u64,
                        stats.f_bavail as u64,
                        stats.f_files as u64,
                        stats.f_ffree as u64,
                        stats.f_bsize as u32,
                        stats.f_namemax as u32,
                        stats.f_frsize as u32,
                    );
                    return;
                }
            }
        }
        reply.statfs(1, 0, 0, 1, 0, 4096, 255, 4096);
    }

    fn readdir(
        &mut self,
        _req: &fuser::Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: fuser::ReplyDirectory,
    ) {
        if !self.inodes.is_valid_inode(ino) {
            reply.error(libc::ENOENT);
            return;
        }

        let entries = self.inodes.readdir(ino);
        let skip = if offset < 0 { 0usize } else { offset as usize };
        for (i, entry) in entries.iter().enumerate().skip(skip) {
            if reply.add(entry.inode, (i + 1) as i64, entry.kind, &entry.name) {
                break;
            }
        }
        reply.ok();
    }

    fn read(
        &mut self,
        _req: &fuser::Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock: Option<u64>,
        reply: fuser::ReplyData,
    ) {
        if let Some(host_path) = self.inodes.passthrough_paths.get(&ino) {
            use std::io::{Read, Seek};
            let mut f = match std::fs::File::open(host_path) {
                Ok(f) => f,
                Err(_) => {
                    reply.error(libc::EIO);
                    return;
                }
            };
            if f.seek(std::io::SeekFrom::Start(offset as u64)).is_err() {
                reply.error(libc::EIO);
                return;
            }
            let mut buf = vec![0u8; size as usize];
            let n = match f.read(&mut buf) {
                Ok(n) => n,
                Err(_) => {
                    reply.error(libc::EIO);
                    return;
                }
            };
            reply.data(&buf[..n]);
            return;
        }

        if let (Some(base), Some(rel_path)) = (&self.write_layer, self.inodes.get_path(ino)) {
            let full_path = base.join(rel_path);
            if full_path.exists() && !full_path.is_dir() {
                use std::io::{Read, Seek};
                let mut f = match std::fs::File::open(full_path) {
                    Ok(f) => f,
                    Err(_) => {
                        reply.error(libc::EIO);
                        return;
                    }
                };
                if f.seek(std::io::SeekFrom::Start(offset as u64)).is_err() {
                    reply.error(libc::EIO);
                    return;
                }
                let mut buf = vec![0u8; size as usize];
                let n = match f.read(&mut buf) {
                    Ok(n) => n,
                    Err(_) => {
                        reply.error(libc::EIO);
                        return;
                    }
                };
                reply.data(&buf[..n]);
                return;
            }
        }
        read::handle_read(self, ino, offset, size, reply);
    }

    fn setattr(
        &mut self,
        _req: &fuser::Request,
        ino: u64,
        mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<fuser::TimeOrNow>,
        _mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<std::time::SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<std::time::SystemTime>,
        _chgtime: Option<std::time::SystemTime>,
        _bkuptime: Option<std::time::SystemTime>,
        _flags: Option<u32>,
        reply: fuser::ReplyAttr,
    ) {
        if self.write_layer.is_none() {
            reply.error(libc::EROFS);
            return;
        }

        if let Err(e) = self.ensure_cow(ino) {
            reply.error(e.raw_os_error().unwrap_or(libc::EIO));
            return;
        }

        if let (Some(base), Some(rel_path)) = (&self.write_layer, self.inodes.get_path(ino)) {
            let full_path = base.join(rel_path);
            if let Some(s) = size {
                let _ = std::fs::File::options()
                    .write(true)
                    .open(&full_path)
                    .and_then(|f| f.set_len(s));
            }
            if let Some(m) = mode {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&full_path, std::fs::Permissions::from_mode(m));
            }
        }

        self.getattr(_req, ino, reply);
    }

    fn write(
        &mut self,
        _req: &fuser::Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock: Option<u64>,
        reply: fuser::ReplyWrite,
    ) {
        if self.write_layer.is_none() {
            reply.error(libc::EROFS);
            return;
        }

        if let Err(e) = self.ensure_cow(ino) {
            reply.error(e.raw_os_error().unwrap_or(libc::EIO));
            return;
        }

        if let (Some(base), Some(rel_path)) = (&self.write_layer, self.inodes.get_path(ino)) {
            use std::io::{Seek, Write};
            let full_path = base.join(rel_path);
            let res: std::io::Result<usize> = (|| {
                let mut f = std::fs::File::options().write(true).open(full_path)?;
                f.seek(std::io::SeekFrom::Start(offset as u64))?;
                f.write(data)
            })();

            match res {
                Ok(n) => reply.written(n as u32),
                Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
            }
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn unlink(
        &mut self,
        _req: &fuser::Request,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        if let Some(base) = &self.write_layer {
            if let Some(parent_path) = self.inodes.get_path(parent) {
                let rel_path = parent_path.join(name);
                let full_path = base.join(&rel_path);
                if full_path.exists() {
                    let _ = std::fs::remove_file(full_path);
                    reply.ok();
                    return;
                }
            }
        }
        reply.error(libc::EROFS);
    }

    fn rmdir(
        &mut self,
        _req: &fuser::Request,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        if let Some(base) = &self.write_layer {
            if let Some(parent_path) = self.inodes.get_path(parent) {
                let rel_path = parent_path.join(name);
                let full_path = base.join(&rel_path);
                if full_path.exists() {
                    let _ = std::fs::remove_dir_all(full_path);
                    reply.ok();
                    return;
                }
            }
        }
        reply.error(libc::EROFS);
    }
}
