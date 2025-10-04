use crate::protocols::source::Source;
use blake3::Hasher;
use std::fs::{self, File};
use std::io::Read;
use std::path::PathBuf;

/// Local filesystem source implementation
pub struct LocalSource;

impl LocalSource {
    /// Create a new local source
    pub fn new(_root: PathBuf) -> Self {
        Self
    }
}

impl Source for LocalSource {
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

    fn read_file(&self, path: &PathBuf) -> std::io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn is_symlink(&self, path: &PathBuf) -> bool {
        fs::symlink_metadata(path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    }

    fn read_link(&self, path: &PathBuf) -> std::io::Result<PathBuf> {
        fs::read_link(path)
    }
}
