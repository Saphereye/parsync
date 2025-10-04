use std::path::PathBuf;

/// Abstraction for reading files and metadata from a source location.
/// 
/// Implementations must be Send + Sync for parallel operations.
pub trait Source: Send + Sync {
    /// Get the hash of a file, returns None if unavailable
    fn get_file_hash(&self, path: &PathBuf) -> Option<String>;

    /// Read file content as bytes
    fn read_file(&self, path: &PathBuf) -> std::io::Result<Vec<u8>>;

    /// Check if path is a symlink
    fn is_symlink(&self, path: &PathBuf) -> bool;

    /// Read symlink target
    fn read_link(&self, path: &PathBuf) -> std::io::Result<PathBuf>;
}
