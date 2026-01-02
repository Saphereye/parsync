use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use super::{FileEntry, StorageBackend, SyncError};

/// Local filesystem backend implementation
pub struct LocalBackend;

impl Default for LocalBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Local filesystem backend implementation.
/// Provides simple list/get/put/delete and optimized copy fallback semantics.
impl LocalBackend {
    pub fn new() -> Self {
        Self
    }
    /// Copy a file from `src` to `dst`, returning the number of bytes copied.
    /// Falls back to streaming copy when `std::fs::copy` fails (e.g., cross-device moves),
    /// using the provided buffer to minimize allocations.
    pub fn copy_file(&self, src: &str, dst: &str, buf: &mut [u8]) -> Result<u64, SyncError> {
        match fs::copy(src, dst) {
            Ok(bytes) => Ok(bytes),
            Err(_) => {
                // Fallback to streaming copy if std::fs::copy fails (e.g., cross-device)
                let mut src_file = fs::File::open(src)?;
                let mut dst_file = fs::File::create(dst)?;
                let mut total_bytes = 0u64;
                loop {
                    let n = src_file.read(buf)?;
                    if n == 0 {
                        break;
                    }
                    dst_file.write_all(&buf[..n])?;
                    total_bytes += n as u64;
                }
                Ok(total_bytes)
            }
        }
    }
}

impl StorageBackend for LocalBackend {
    fn list(&self, path: &str) -> Result<Vec<FileEntry>, SyncError> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            entries.push(FileEntry {
                path: entry.path().to_string_lossy().to_string(),
                metadata,
            });
        }
        Ok(entries)
    }

    fn get(&self, path: &str) -> Result<Vec<u8>, SyncError> {
        let mut file = fs::File::open(path)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn put(&self, path: &str, data: &[u8]) -> Result<(), SyncError> {
        let mut file = fs::File::create(path)?;
        file.write_all(data)?;
        Ok(())
    }

    /// Delete a local file or directory.
    /// Directories are removed recursively via `remove_dir_all`; files via `remove_file`.
    fn delete(&self, path: &str) -> Result<(), SyncError> {
        if Path::new(path).is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    fn exists(&self, path: &str) -> Result<bool, SyncError> {
        Ok(Path::new(path).exists())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
