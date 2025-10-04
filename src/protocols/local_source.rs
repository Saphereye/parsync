use crate::protocols::source::Source;
use blake3::Hasher;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::PathBuf;

/// Local filesystem source implementation
pub struct LocalSource {
    #[allow(dead_code)]
    root: PathBuf,
}

impl LocalSource {
    /// Create a new local source at the given root path
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    #[allow(dead_code)]
    pub fn root(&self) -> &PathBuf {
        &self.root
    }
}

impl Source for LocalSource {
    fn list_files(
        &self,
        _include_regex: Option<String>,
        _exclude_regex: Option<String>,
    ) -> Vec<(PathBuf, u64)> {
        // This method is not used in the new design, filtering happens at a higher level
        vec![]
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

    fn get_file_hashes(&self, paths: &[PathBuf]) -> HashMap<PathBuf, String> {
        use rayon::prelude::*;
        
        paths
            .par_iter()
            .filter_map(|path| {
                self.get_file_hash(path).map(|hash| (path.clone(), hash))
            })
            .collect()
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
