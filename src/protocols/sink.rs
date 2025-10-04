use std::collections::HashMap;
use std::path::PathBuf;

/// Abstraction for writing files to a destination location.
/// 
/// Implementations must be Send + Sync for parallel operations.
pub trait Sink: Send + Sync {
    /// Check if file exists at destination
    fn file_exists(&self, path: &PathBuf) -> bool;

    /// Get the hash of a file, returns None if unavailable
    fn get_file_hash(&self, path: &PathBuf) -> Option<String>;

    /// Get hashes for multiple files (can be optimized for remote sinks)
    #[allow(dead_code)]
    fn get_file_hashes(&self, paths: &[PathBuf]) -> HashMap<PathBuf, String> {
        paths
            .iter()
            .filter_map(|path| {
                self.get_file_hash(path).map(|hash| (path.clone(), hash))
            })
            .collect()
    }

    /// Write file to destination, creates parent directories as needed
    #[allow(dead_code)]
    fn write_file(&self, path: &PathBuf, content: &[u8]) -> std::io::Result<()>;

    /// Create directory at destination (like mkdir -p)
    fn create_dir(&self, path: &PathBuf) -> std::io::Result<()>;

    /// Create symlink at destination
    fn create_symlink(&self, target: &PathBuf, link: &PathBuf) -> std::io::Result<()>;

    /// Copy file from source to destination
    fn copy_file(&self, source_path: &PathBuf, dest_path: &PathBuf) -> std::io::Result<()>;
}
