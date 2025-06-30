use crate::protocols::traits::{Sink, Source};
use crate::protocols::Protocol;
use crate::protocols::metadata::{FileEntry, FileKind, FileMetadata};
use blake3::Hasher;
use rayon::prelude::*;
use regex::Regex;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::PathBuf;
use walkdir::WalkDir;

pub struct LocalProtocol;

// Helps to inforce that LocalProtocol implements both Source and Sink traits
impl Protocol for LocalProtocol {}

impl Source for LocalProtocol {
    type Path = PathBuf;

    fn list_files(
        &self,
        base: &Self::Path,
        include_regex: Option<&str>,
        exclude_regex: Option<&str>,
    ) -> anyhow::Result<Vec<(FileEntry<Self::Path>, u64)>> {
        let include = include_regex.map(|r| Regex::new(r).unwrap());
        let exclude = exclude_regex.map(|r| Regex::new(r).unwrap());

        let files = WalkDir::new(base)
            .follow_links(true)
            .into_iter()
            .filter_map(Result::ok)
            .par_bridge()
            .filter_map(|entry| {
                let path = entry.path().to_path_buf();
                let path_str = path.to_string_lossy();

                let is_file = entry.file_type().is_file();
                let is_symlink = entry.file_type().is_symlink();
                let is_dir = entry.file_type().is_dir();

                if !is_file && !is_symlink && !is_dir {
                    return None;
                }

                let include_ok = include
                    .as_ref()
                    .map(|r| r.is_match(&path_str))
                    .unwrap_or(true);
                let exclude_ok = exclude
                    .as_ref()
                    .map(|r| !r.is_match(&path_str))
                    .unwrap_or(true);

                if !include_ok || !exclude_ok {
                    return None;
                }

                let kind = if is_file {
                    FileKind::File
                } else if is_dir {
                    FileKind::Directory
                } else if is_symlink {
                    match fs::read_link(&path) {
                        Ok(target) => FileKind::Symlink(target),
                        Err(_) => return None, // skip broken symlinks or unreadable ones
                    }
                } else {
                    return None;
                };

                let size = match &kind {
                    FileKind::File => fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
                    FileKind::Symlink(_) => {
                        fs::symlink_metadata(&path).map(|m| m.len()).unwrap_or(0)
                    }
                    FileKind::Directory => 0,
                };

                let file_entry = FileEntry { path, kind, size };

                Some((file_entry, size))
            })
            .collect();

        Ok(files)
    }

    fn read_file(&self, path: &Self::Path) -> anyhow::Result<Box<dyn std::io::Read + Send>> {
        Ok(Box::new(File::open(path)?))
    }

    fn get_metadata(&self, path: &Self::Path) -> anyhow::Result<FileMetadata> {
        let size = fs::metadata(path)?.len();
        let checksum = Self::file_checksum(path);
        Ok(FileMetadata { size, checksum })
    }
}

impl LocalProtocol {
    fn file_checksum(path: &PathBuf) -> Option<String> {
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
}

impl Sink for LocalProtocol {
    type Path = PathBuf;

    fn write_file(
        &self,
        path: &Self::Path,
        reader: &mut dyn Read,
        _size: u64,
    ) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        std::io::copy(reader, &mut file)?;
        Ok(())
    }

    fn file_exists(&self, path: &Self::Path) -> anyhow::Result<bool> {
        Ok(path.try_exists()?)
    }

    fn compare_metadata(&self, path: &Self::Path, meta: &FileMetadata) -> bool {
        if let Ok(actual_meta) = self.get_metadata(path) {
            actual_meta.size == meta.size && actual_meta.checksum == meta.checksum
        } else {
            false
        }
    }

    fn create_symlink(&self, target: &Self::Path, link: &Self::Path) -> std::io::Result<()> {
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

    fn create_dir(&self, path: &Self::Path) -> std::io::Result<()> {
        fs::create_dir_all(path)
    }
}
