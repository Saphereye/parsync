use std::collections::HashMap;
use std::path::PathBuf;

/// Trait for writing files to a destination location
pub trait Sink: Send + Sync {
    /// Check if a file exists at the destination
    fn file_exists(&self, path: &PathBuf) -> bool;

    /// Get the checksum/hash of a file at the given path
    fn get_file_hash(&self, path: &PathBuf) -> Option<String>;

    /// Get checksums for multiple files at once (can be optimized for remote sinks)
    fn get_file_hashes(&self, paths: &[PathBuf]) -> HashMap<PathBuf, String> {
        paths
            .iter()
            .filter_map(|path| {
                self.get_file_hash(path).map(|hash| (path.clone(), hash))
            })
            .collect()
    }

    /// Write a file to the destination
    fn write_file(&self, path: &PathBuf, content: &[u8]) -> std::io::Result<()>;

    /// Create a directory at the destination
    fn create_dir(&self, path: &PathBuf) -> std::io::Result<()>;

    /// Create a symlink at the destination
    fn create_symlink(&self, target: &PathBuf, link: &PathBuf) -> std::io::Result<()>;

    /// Copy a file from source to destination (can be optimized for local-to-local copies)
    fn copy_file(&self, source_path: &PathBuf, dest_path: &PathBuf) -> std::io::Result<()>;
}
