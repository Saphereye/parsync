use crate::protocols::sink::Sink;
use blake3::Hasher;
use std::fs::{self, File};
use std::io::Read;
use std::path::PathBuf;

/// Local filesystem sink implementation
pub struct LocalSink;

impl LocalSink {
    /// Create a new local sink
    pub fn new(_root: PathBuf) -> Self {
        Self
    }
}

impl Sink for LocalSink {
    fn file_exists(&self, path: &PathBuf) -> bool {
        path.exists()
    }

    fn get_file_hash(&self, path: &PathBuf) -> Option<String> {
        let mut file = File::open(path).ok()?;
        let mut hasher = Hasher::new();
        let mut buffer = [0; 8192];

        while let Ok(n) = file.read(&mut buffer) {
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        Some(hasher.finalize().to_hex().to_string())
    }

    fn create_dir(&self, path: &PathBuf) -> std::io::Result<()> {
        fs::create_dir_all(path)
    }

    fn create_symlink(&self, target: &PathBuf, link: &PathBuf) -> std::io::Result<()> {
        if let Some(parent) = link.parent() {
            fs::create_dir_all(parent)?;
        }

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link)
        }

        #[cfg(windows)]
        {
            use std::os::windows::fs::{symlink_dir, symlink_file};
            if target.is_dir() {
                symlink_dir(target, link)
            } else {
                symlink_file(target, link)
            }
        }

        #[cfg(not(any(unix, windows)))]
        {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "symlink creation not supported on this platform",
            ))
        }
    }

    fn copy_file(&self, source_path: &PathBuf, dest_path: &PathBuf) -> std::io::Result<()> {
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        fs::copy(source_path, dest_path)?;
        Ok(())
    }
}
