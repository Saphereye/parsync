use std::collections::HashMap;
use std::path::PathBuf;

/// Trait for reading files and metadata from a source location
pub trait Source: Send + Sync {
    /// List all files at the source location that match the given filters
    fn list_files(
        &self,
        include_regex: Option<String>,
        exclude_regex: Option<String>,
    ) -> Vec<(PathBuf, u64)>;

    /// Get the checksum/hash of a file at the given path
    fn get_file_hash(&self, path: &PathBuf) -> Option<String>;

    /// Get checksums for multiple files at once (can be optimized for remote sources)
    fn get_file_hashes(&self, paths: &[PathBuf]) -> HashMap<PathBuf, String> {
        paths
            .iter()
            .filter_map(|path| {
                self.get_file_hash(path).map(|hash| (path.clone(), hash))
            })
            .collect()
    }

    /// Read a file's content for copying
    fn read_file(&self, path: &PathBuf) -> std::io::Result<Vec<u8>>;

    /// Check if a file is a symlink
    fn is_symlink(&self, path: &PathBuf) -> bool;

    /// Read the target of a symlink
    fn read_link(&self, path: &PathBuf) -> std::io::Result<PathBuf>;
}
