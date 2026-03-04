//! FUSE filesystem implementation for Hexz archives.

mod read;

use fuser::Filesystem;
use hexz_core::Archive;
use hexz_vfs::{InodeMap, VfsAttr, DirEntry};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub(crate) const TTL: Duration = Duration::from_secs(1);

fn vfs_attr_to_fuser(attr: VfsAttr) -> fuser::FileAttr {
    fuser::FileAttr {
        ino: attr.ino,
        size: attr.size,
        blocks: attr.blocks,
        atime: attr.atime,
        mtime: attr.mtime,
        ctime: attr.ctime,
        crtime: attr.crtime,
        kind: attr.kind,
        perm: attr.perm,
        nlink: attr.nlink,
        uid: attr.uid,
        gid: attr.gid,
        rdev: attr.rdev,
        blksize: attr.blksize,
        flags: attr.flags,
    }
}

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

    fn is_whiteout(&self, parent_ino: u64, name: &std::ffi::OsStr) -> bool {
        let base = match &self.write_layer {
            Some(b) => b,
            None => return false,
        };
        let parent_path = match self.inodes.get_path(parent_ino) {
            Some(p) => p,
            None => return false,
        };
        let whiteout_path = base.join(parent_path).join(".hexz_whiteout").join(name);
        whiteout_path.exists()
    }

    fn create_whiteout(&self, parent_ino: u64, name: &std::ffi::OsStr) -> std::io::Result<()> {
        let base = match &self.write_layer {
            Some(b) => b,
            None => return Err(std::io::Error::from_raw_os_error(libc::EROFS)),
        };
        let parent_path = self.inodes.get_path(parent_ino).ok_or_else(|| std::io::Error::from_raw_os_error(libc::ENOENT))?;
        let whiteout_dir = base.join(parent_path).join(".hexz_whiteout");
        std::fs::create_dir_all(&whiteout_dir)?;
        let whiteout_path = whiteout_dir.join(name);
        std::fs::File::create(whiteout_path)?;
        Ok(())
    }

    fn remove_whiteout(&self, parent_ino: u64, name: &std::ffi::OsStr) -> std::io::Result<()> {
        let base = match &self.write_layer {
            Some(b) => b,
            None => return Ok(()),
        };
        let parent_path = self.inodes.get_path(parent_ino).ok_or_else(|| std::io::Error::from_raw_os_error(libc::ENOENT))?;
        let whiteout_path = base.join(parent_path).join(".hexz_whiteout").join(name);
        if whiteout_path.exists() {
            std::fs::remove_file(whiteout_path)?;
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
            if let Ok(meta) = std::fs::symlink_metadata(&full_path) {
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
                    } else if meta.file_type().is_symlink() {
                        fuser::FileType::Symlink
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

        let mut attr = vfs_attr_to_fuser(self.inodes.getattr(ino));
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
        if self.is_whiteout(parent, name) {
            reply.error(libc::ENOENT);
            return;
        }

        if let Some(parent_path) = self.inodes.get_path(parent) {
            let rel_path = parent_path.join(name);

            // 1. Check overlay first (including newly created files)
            if let Some(base) = &self.write_layer {
                let full_path = base.join(&rel_path);
                if full_path.exists() || full_path.is_symlink() {
                    let ino = self.inodes.get_inode(&rel_path).unwrap_or_else(|| {
                        self.inodes.add_file_at_path(&rel_path, full_path.is_dir())
                    });
                    let attr = self.getattr_internal(ino);
                    reply.entry(&TTL, &attr, 0);
                    return;
                }
            }

            // 2. Check archive
            if let Some(inode) = self.inodes.get_inode(&rel_path) {
                let attr = self.getattr_internal(inode);
                reply.entry(&TTL, &attr, 0);
                return;
            }
        }

        reply.error(libc::ENOENT);
    }

    fn open(&mut self, _req: &fuser::Request, ino: u64, _flags: i32, reply: fuser::ReplyOpen) {
        if self.write_layer.is_some() {
            let _ = self.ensure_cow(ino);
        }
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
        mode: u32,
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

                let _ = self.remove_whiteout(parent, name);

                match std::fs::File::create(&full_path) {
                    Ok(_) => {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = std::fs::set_permissions(&full_path, std::fs::Permissions::from_mode(mode));
                        
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
        mode: u32,
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

                let _ = self.remove_whiteout(parent, name);

                match std::fs::File::create(&full_path) {
                    Ok(_) => {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = std::fs::set_permissions(&full_path, std::fs::Permissions::from_mode(mode));
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
        mode: u32,
        _umask: u32,
        reply: fuser::ReplyEntry,
    ) {
        if let Some(base) = &self.write_layer {
            if let Some(parent_path) = self.inodes.get_path(parent) {
                let rel_path = parent_path.join(name);
                let full_path = base.join(&rel_path);
                
                let _ = self.remove_whiteout(parent, name);

                match std::fs::create_dir_all(&full_path) {
                    Ok(_) => {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = std::fs::set_permissions(&full_path, std::fs::Permissions::from_mode(mode));
                        let ino = self.inodes.add_file_at_path(&rel_path, true);
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

        let mut entries = self.inodes.readdir(ino);
        
        // Filter out whiteouts and add files from overlay
        if let (Some(base), Some(parent_path)) = (&self.write_layer, self.inodes.get_path(ino)) {
            let full_parent_path = base.join(&parent_path);
            
            // 1. Add files from overlay that aren't in archive
            if let Ok(dir) = std::fs::read_dir(&full_parent_path) {
                for entry in dir.flatten() {
                    let name = entry.file_name();
                    if name == ".hexz_whiteout" { continue; }
                    let name_str = name.to_string_lossy().to_string();
                    if !entries.iter().any(|e| e.name == name_str) {
                        let rel_path = parent_path.join(&name);
                        let child_ino = self.inodes.add_file_at_path(&rel_path, entry.path().is_dir());
                        let kind = if entry.path().is_dir() { fuser::FileType::Directory } else { fuser::FileType::RegularFile };
                        entries.push(DirEntry { inode: child_ino, kind, name: name_str });
                    }
                }
            }

            // 2. Remove entries that are whiteouted
            entries.retain(|e| {
                if e.name == "." || e.name == ".." { return true; }
                !self.is_whiteout(ino, std::ffi::OsStr::new(&e.name))
            });
        }

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
                
                // If in overlay, remove it
                if full_path.exists() || full_path.is_symlink() {
                    let _ = std::fs::remove_file(full_path);
                }
                
                self.inodes.remove_path(&rel_path);

                // Always create whiteout if it exists in archive
                if self.inodes.get_inode(&rel_path).is_some() {
                    if let Err(e) = self.create_whiteout(parent, name) {
                        reply.error(e.raw_os_error().unwrap_or(libc::EIO));
                        return;
                    }
                }
                
                reply.ok();
                return;
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
                    if let Err(e) = std::fs::remove_dir_all(full_path) {
                        reply.error(e.raw_os_error().unwrap_or(libc::EIO));
                        return;
                    }
                }

                self.inodes.remove_path(&rel_path);

                if self.inodes.get_inode(&rel_path).is_some() {
                    if let Err(e) = self.create_whiteout(parent, name) {
                        reply.error(e.raw_os_error().unwrap_or(libc::EIO));
                        return;
                    }
                }

                reply.ok();
                return;
            }
        }
        reply.error(libc::EROFS);
    }

    fn rename(
        &mut self,
        _req: &fuser::Request,
        parent: u64,
        name: &std::ffi::OsStr,
        newparent: u64,
        newname: &std::ffi::OsStr,
        _flags: u32,
        reply: fuser::ReplyEmpty,
    ) {
        if self.write_layer.is_none() {
            reply.error(libc::EROFS);
            return;
        }

        if let (Some(base), Some(old_p), Some(new_p)) = (
            &self.write_layer,
            self.inodes.get_path(parent),
            self.inodes.get_path(newparent),
        ) {
            let old_rel = old_p.join(name);
            let new_rel = new_p.join(newname);
            let old_full = base.join(&old_rel);
            let new_full = base.join(&new_rel);

            // Ensure source is in overlay
            if !old_full.exists() {
                if let Some(ino) = self.inodes.get_inode(&old_rel) {
                    if let Err(e) = self.ensure_cow(ino) {
                        reply.error(e.raw_os_error().unwrap_or(libc::EIO));
                        return;
                    }
                } else {
                    reply.error(libc::ENOENT);
                    return;
                }
            }

            if let Some(p) = new_full.parent() {
                let _ = std::fs::create_dir_all(p);
            }

            match std::fs::rename(old_full, new_full) {
                Ok(_) => {
                    self.inodes.rename_path(&old_rel, &new_rel);
                    let _ = self.create_whiteout(parent, name);
                    let _ = self.remove_whiteout(newparent, newname);
                    reply.ok();
                }
                Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
            }
        } else {
            reply.error(libc::EROFS);
        }
    }

    fn link(
        &mut self,
        _req: &fuser::Request,
        ino: u64,
        newparent: u64,
        newname: &std::ffi::OsStr,
        reply: fuser::ReplyEntry,
    ) {
        if self.write_layer.is_none() {
            reply.error(libc::EROFS);
            return;
        }

        if let Err(e) = self.ensure_cow(ino) {
            reply.error(e.raw_os_error().unwrap_or(libc::EIO));
            return;
        }

        if let (Some(base), Some(old_rel), Some(new_parent_rel)) = (
            &self.write_layer,
            self.inodes.get_path(ino),
            self.inodes.get_path(newparent),
        ) {
            let new_rel = new_parent_rel.join(newname);
            let old_full = base.join(old_rel);
            let new_full = base.join(&new_rel);

            match std::fs::hard_link(old_full, new_full) {
                Ok(_) => {
                    // In our current InodeMap, we don't support true hardlinks (1 ino -> many paths)
                    // perfectly, but we can add the new path pointing to the same Inode.
                    // This is enough for most tools.
                    let _ = self.inodes.add_file_at_path(&new_rel, false);
                    let attr = self.getattr_internal(ino);
                    reply.entry(&TTL, &attr, 0);
                }
                Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
            }
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn symlink(
        &mut self,
        _req: &fuser::Request,
        parent: u64,
        name: &std::ffi::OsStr,
        link: &std::path::Path,
        reply: fuser::ReplyEntry,
    ) {
        if let Some(base) = &self.write_layer {
            if let Some(parent_path) = self.inodes.get_path(parent) {
                let rel_path = parent_path.join(name);
                let full_path = base.join(&rel_path);
                
                if let Some(p) = full_path.parent() {
                    let _ = std::fs::create_dir_all(p);
                }

                let _ = self.remove_whiteout(parent, name);

                #[cfg(unix)]
                match std::os::unix::fs::symlink(link, &full_path) {
                    Ok(_) => {
                        let ino = self.inodes.add_file_at_path(&rel_path, false);
                        let attr = self.getattr_internal(ino);
                        reply.entry(&TTL, &attr, 0);
                    }
                    Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
                }
                #[cfg(not(unix))]
                reply.error(libc::ENOSYS);
                return;
            }
        }
        reply.error(libc::EROFS);
    }

    fn readlink(&mut self, _req: &fuser::Request, ino: u64, reply: fuser::ReplyData) {
        if let (Some(base), Some(rel_path)) = (&self.write_layer, self.inodes.get_path(ino)) {
            let full_path = base.join(rel_path);
            match std::fs::read_link(full_path) {
                Ok(link) => {
                    use std::os::unix::ffi::OsStrExt;
                    reply.data(link.as_os_str().as_bytes());
                },
                Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
            }
        } else {
            reply.error(libc::EINVAL);
        }
    }
}
