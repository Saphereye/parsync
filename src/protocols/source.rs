use std::collections::HashMap;
use std::path::PathBuf;

/// Trait for reading files and metadata from a source location.
/// 
/// This trait provides an abstraction for reading files from various sources,
/// such as local filesystems, SSH servers, cloud storage, etc.
/// 
/// # Thread Safety
/// Implementations must be Send + Sync as they are used in parallel operations.
/// 
/// # Examples
/// 
/// ```ignore
/// use parsync::protocols::source::Source;
/// use parsync::protocols::local_source::LocalSource;
/// use std::path::PathBuf;
/// 
/// let source = LocalSource::new(PathBuf::from("/path/to/source"));
/// let hash = source.get_file_hash(&PathBuf::from("/path/to/file.txt"));
/// ```
pub trait Source: Send + Sync {
    /// List all files at the source location that match the given filters
    fn list_files(
        &self,
        include_regex: Option<String>,
        exclude_regex: Option<String>,
    ) -> Vec<(PathBuf, u64)>;

    /// Get the checksum/hash of a file at the given path.
    /// 
    /// Returns `None` if the file cannot be read or hashed.
    fn get_file_hash(&self, path: &PathBuf) -> Option<String>;

    /// Get checksums for multiple files at once.
    /// 
    /// This method can be overridden for optimized batch operations,
    /// particularly useful for remote sources where network calls can be batched.
    fn get_file_hashes(&self, paths: &[PathBuf]) -> HashMap<PathBuf, String> {
        paths
            .iter()
            .filter_map(|path| {
                self.get_file_hash(path).map(|hash| (path.clone(), hash))
            })
            .collect()
    }

    /// Read a file's content for copying.
    /// 
    /// Returns the entire file content as bytes.
    fn read_file(&self, path: &PathBuf) -> std::io::Result<Vec<u8>>;

    /// Check if a file is a symlink
    fn is_symlink(&self, path: &PathBuf) -> bool;

    /// Read the target of a symlink.
    /// 
    /// Returns an error if the path is not a symlink or cannot be read.
    fn read_link(&self, path: &PathBuf) -> std::io::Result<PathBuf>;
}
