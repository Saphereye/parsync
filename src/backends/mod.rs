#[allow(dead_code)]
pub mod local;

use std::fs;

#[allow(dead_code, unused)]
#[derive(Debug)]
pub enum SyncError {
    Io(std::io::Error),
    NotFound(String),
    Other(String),
}

impl From<std::io::Error> for SyncError {
    fn from(e: std::io::Error) -> Self {
        SyncError::Io(e)
    }
}

#[allow(dead_code, unused)]
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub metadata: fs::Metadata,
}

#[allow(dead_code, unused)]
/// StorageBackend defines the abstract operations over a storage system.
/// Implementations must be `Send + Sync + Any` and safe to use across threads.
///
/// Contract:
/// - `list`: Return entries for a given path without side effects.
/// - `get`: Read a file's bytes from the backend.
/// - `put`: Write bytes to a destination path (create parents as needed).
/// - `delete`: Remove a file or directory (recursive for directories).
/// - `exists`: Return whether the path exists.
/// - `as_any`: Downcast support for concrete backend-specific behavior.
pub trait StorageBackend: Send + Sync + std::any::Any {
    /// List files and directories at `path`.
    /// Returns a vector of `FileEntry` describing the found items.
    fn list(&self, path: &str) -> Result<Vec<FileEntry>, SyncError>;
    /// Read and return the full contents of the file at `path` as bytes.
    fn get(&self, path: &str) -> Result<Vec<u8>, SyncError>;
    /// Write `data` to the file at `path`, creating parents if necessary.
    fn put(&self, path: &str, data: &[u8]) -> Result<(), SyncError>;
    /// Delete the file or directory at `path`. Implementations should handle
    /// recursive deletion for directories.
    fn delete(&self, path: &str) -> Result<(), SyncError>;
    /// Return `true` if `path` exists in the backend, `false` otherwise.
    fn exists(&self, path: &str) -> Result<bool, SyncError>;
    /// Return a `&dyn Any` reference for downcasting to the concrete backend type.
    fn as_any(&self) -> &dyn std::any::Any;
}

pub use local::LocalBackend;

/// Given a protocol-prefixed path, returns (Box<dyn StorageBackend>, normalized_path).
/// Example: "file:///tmp/foo" -> (LocalBackend, "/tmp/foo")
use std::sync::Arc;

#[allow(dead_code, unused)]
pub fn backend_and_path(
    url: &str,
) -> Result<(Arc<dyn StorageBackend + Send + Sync>, &str), SyncError> {
    if let Some(idx) = url.find("://") {
        let (proto, rest) = url.split_at(idx);
        let path = &rest[3..];
        match proto {
            "file" => Ok((Arc::new(LocalBackend::new()), path)),
            // "ssh" | "sftp" => Ok((Arc::new(SshBackend::new()), path)), // Placeholder for future
            _ => Err(SyncError::Other(format!("Unsupported protocol: {}", proto))),
        }
    } else {
        // Default to local file if no protocol specified
        Ok((Arc::new(LocalBackend::new()), url))
    }
}
