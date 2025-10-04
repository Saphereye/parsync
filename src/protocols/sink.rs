use std::collections::HashMap;
use std::path::PathBuf;

/// Trait for writing files to a destination location.
/// 
/// This trait provides an abstraction for writing files to various destinations,
/// such as local filesystems, SSH servers, cloud storage, etc.
/// 
/// # Thread Safety
/// Implementations must be Send + Sync as they are used in parallel operations.
/// 
/// # Examples
/// 
/// ```ignore
/// use parsync::protocols::sink::Sink;
/// use parsync::protocols::local_sink::LocalSink;
/// use std::path::PathBuf;
/// 
/// let sink = LocalSink::new(PathBuf::from("/path/to/destination"));
/// let exists = sink.file_exists(&PathBuf::from("/path/to/file.txt"));
/// ```
pub trait Sink: Send + Sync {
    /// Check if a file exists at the destination
    fn file_exists(&self, path: &PathBuf) -> bool;

    /// Get the checksum/hash of a file at the given path.
    /// 
    /// Returns `None` if the file cannot be read or hashed.
    fn get_file_hash(&self, path: &PathBuf) -> Option<String>;

    /// Get checksums for multiple files at once.
    /// 
    /// This method can be overridden for optimized batch operations,
    /// particularly useful for remote sinks where network calls can be batched.
    #[allow(dead_code)]
    fn get_file_hashes(&self, paths: &[PathBuf]) -> HashMap<PathBuf, String> {
        paths
            .iter()
            .filter_map(|path| {
                self.get_file_hash(path).map(|hash| (path.clone(), hash))
            })
            .collect()
    }

    /// Write a file to the destination.
    /// 
    /// This method should create parent directories as needed.
    #[allow(dead_code)]
    fn write_file(&self, path: &PathBuf, content: &[u8]) -> std::io::Result<()>;

    /// Create a directory at the destination.
    /// 
    /// Should create parent directories as needed (like `mkdir -p`).
    fn create_dir(&self, path: &PathBuf) -> std::io::Result<()>;

    /// Create a symlink at the destination.
    /// 
    /// # Arguments
    /// * `target` - The target path the symlink should point to
    /// * `link` - The path where the symlink should be created
    fn create_symlink(&self, target: &PathBuf, link: &PathBuf) -> std::io::Result<()>;

    /// Copy a file from source to destination.
    /// 
    /// This method can be optimized for local-to-local copies to avoid
    /// reading the entire file into memory.
    fn copy_file(&self, source_path: &PathBuf, dest_path: &PathBuf) -> std::io::Result<()>;
}
