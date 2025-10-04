use std::path::PathBuf;

/// Abstraction for writing files to a destination location.
/// 
/// Implementations must be Send + Sync for parallel operations.
pub trait Sink: Send + Sync {
    /// Check if file exists at destination
    fn file_exists(&self, path: &PathBuf) -> bool;

    /// Get the hash of a file, returns None if unavailable
    fn get_file_hash(&self, path: &PathBuf) -> Option<String>;

    /// Create directory at destination (like mkdir -p)
    fn create_dir(&self, path: &PathBuf) -> std::io::Result<()>;

    /// Create symlink at destination
    fn create_symlink(&self, target: &PathBuf, link: &PathBuf) -> std::io::Result<()>;

    /// Copy file from source to destination
    fn copy_file(&self, source_path: &PathBuf, dest_path: &PathBuf) -> std::io::Result<()>;
}
