//! Local file storage backend using position-independent I/O (`pread`).

use bytes::{Bytes, BytesMut};
use hexz_common::Result;
use hexz_core::store::StorageBackend;
use std::fs::File;

#[cfg(unix)]
fn read_exact_at(file: &File, buffer: &mut [u8], offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.read_exact_at(buffer, offset)
}

#[cfg(windows)]
fn read_exact_at(file: &File, buffer: &mut [u8], mut offset: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    let mut pos = 0;
    while pos < buffer.len() {
        let n = file.seek_read(&mut buffer[pos..], offset)?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "unexpected eof",
            ));
        }
        pos += n;
        offset += n as u64;
    }
    Ok(())
}

/// Storage backend backed by a local file using `pread(2)` for lock-free concurrent reads.
#[derive(Debug)]
pub struct FileBackend {
    inner: File,
    size: u64,
}

impl FileBackend {
    /// Opens the file at `path` read-only and caches its size.
    pub fn new(path: &std::path::Path) -> Result<Self> {
        let file = File::open(path)?;
        let metadata = file.metadata()?;
        Ok(Self {
            inner: file,
            size: metadata.len(),
        })
    }
}

impl StorageBackend for FileBackend {
    fn read_exact(&self, offset: u64, len: usize) -> Result<Bytes> {
        let mut buffer = BytesMut::with_capacity(len);
        // SAFETY: read_exact_at initialises all bytes before we read them.
        unsafe { buffer.set_len(len) }
        match read_exact_at(&self.inner, &mut buffer, offset) {
            Ok(_) => Ok(buffer.freeze()),
            Err(e) => Err(hexz_common::Error::Io(e)),
        }
    }

    fn len(&self) -> u64 {
        self.size
    }
}
