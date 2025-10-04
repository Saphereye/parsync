use std::collections::HashMap;
use std::path::PathBuf;

/// Abstraction for reading files and metadata from a source location.
/// 
/// Implementations must be Send + Sync for parallel operations.
pub trait Source: Send + Sync {
    /// List files matching the given filters
    #[allow(dead_code)]
    fn list_files(
        &self,
        include_regex: Option<String>,
        exclude_regex: Option<String>,
    ) -> Vec<(PathBuf, u64)>;

    /// Get the hash of a file, returns None if unavailable
    fn get_file_hash(&self, path: &PathBuf) -> Option<String>;

    /// Get hashes for multiple files (can be optimized for remote sources)
    #[allow(dead_code)]
    fn get_file_hashes(&self, paths: &[PathBuf]) -> HashMap<PathBuf, String> {
        paths
            .iter()
            .filter_map(|path| {
                self.get_file_hash(path).map(|hash| (path.clone(), hash))
            })
            .collect()
    }

    /// Read file content as bytes
    fn read_file(&self, path: &PathBuf) -> std::io::Result<Vec<u8>>;

    /// Check if path is a symlink
    fn is_symlink(&self, path: &PathBuf) -> bool;

    /// Read symlink target
    fn read_link(&self, path: &PathBuf) -> std::io::Result<PathBuf>;
}
