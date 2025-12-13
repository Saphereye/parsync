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
pub trait StorageBackend: Send + Sync + std::any::Any {
    fn list(&self, path: &str) -> Result<Vec<FileEntry>, SyncError>;
    fn get(&self, path: &str) -> Result<Vec<u8>, SyncError>;
    fn put(&self, path: &str, data: &[u8]) -> Result<(), SyncError>;
    fn delete(&self, path: &str) -> Result<(), SyncError>;
    fn exists(&self, path: &str) -> Result<bool, SyncError>;
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
