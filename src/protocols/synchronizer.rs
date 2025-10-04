use crate::protocols::sink::Sink;
use crate::protocols::source::Source;
use crate::utils::Status;
use indicatif::ProgressBar;
use log::{debug, error};
use rayon::prelude::*;
use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::ops::Not;
use std::path::PathBuf;
use walkdir::WalkDir;

/// Orchestrates file synchronization between any Source and Sink.
/// 
/// Uses optimized hash comparison: fetches hashes from destination,
/// compares at source, and transfers only necessary files.
pub struct Synchronizer<S: Source, D: Sink> {
    source: S,
    sink: D,
}

impl<S: Source, D: Sink> Synchronizer<S, D> {
    /// Create a new synchronizer with the given source and sink
    pub fn new(source: S, sink: D) -> Self {
        Self { source, sink }
    }

    /// Get list of files that need to be synced
    /// 
    /// Fetches hashes from destination, compares at source,
    /// and returns only files that are missing or have different hashes
    pub fn get_files_to_sync(
        &self,
        source_root: &PathBuf,
        dest_root: &PathBuf,
        include_regex: Option<String>,
        exclude_regex: Option<String>,
        no_verify: bool,
    ) -> Vec<(PathBuf, u64)> {
        let include = include_regex.map(|r| Regex::new(&r).unwrap());
        let exclude = exclude_regex.map(|r| Regex::new(&r).unwrap());

        let files: Vec<_> = WalkDir::new(source_root)
            .follow_links(true)
            .into_iter()
            .filter_map(Result::ok)
            .par_bridge()
            .filter_map(|e| {
                let path = e.path();
                let path_str = path.to_string_lossy();

                let is_symlink = e.file_type().is_symlink();
                let is_file = e.file_type().is_file();
                let is_dir = e.file_type().is_dir();
                let is_empty_dir = is_dir
                    && path
                        .read_dir()
                        .map(|mut i| i.next().is_none())
                        .unwrap_or(false);

                if !(is_file || is_symlink || is_empty_dir) {
                    return None;
                }

                if include
                    .as_ref()
                    .map(|r| r.is_match(&path_str))
                    .unwrap_or(true)
                    && !exclude
                        .as_ref()
                        .map(|r| r.is_match(&path_str))
                        .unwrap_or(false)
                {
                    // New hash comparison logic: fetch hash from destination first
                    if !no_verify && is_file {
                        if let Ok(relative) = path.strip_prefix(source_root) {
                            let dest_path = dest_root.join(relative);
                            
                            // Check if file exists at destination
                            if self.sink.file_exists(&dest_path) {
                                // Get hash from destination
                                if let Some(dest_hash) = self.sink.get_file_hash(&dest_path) {
                                    // Get hash from source and compare
                                    if let Some(src_hash) = self.source.get_file_hash(&path.to_path_buf()) {
                                        if src_hash == dest_hash {
                                            // Hashes match, skip this file
                                            return None;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    let size = if is_dir {
                        0
                    } else if is_symlink {
                        std::fs::symlink_metadata(path).map(|m| m.len()).unwrap_or(0)
                    } else {
                        std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
                    };

                    Some((path.to_path_buf(), size))
                } else {
                    None
                }
            })
            .collect();

        files
    }

    /// Sync files from source to sink with parallel execution
    pub fn sync_files(
        &self,
        files: &[(PathBuf, u64)],
        source_root: &PathBuf,
        dest_root: &PathBuf,
        pb: &Option<ProgressBar>,
        dry_run: bool,
    ) {
        files.par_iter().for_each(|(file, size)| {
            let rel_path = file.strip_prefix(source_root).unwrap();
            let dest_file = dest_root.join(rel_path);

            // Handle empty directories
            if size == &0 && file.is_dir() {
                if dry_run {
                    debug!("Dry-run: Would create empty directory {:?}", dest_file);
                } else if let Err(e) = self.sink.create_dir(&dest_file) {
                    error!("Failed to create directory {:?}: {}", dest_file, e);
                } else {
                    debug!("Created directory {:?}", dest_file);
                }
                return;
            }

            // Handle symlinks
            if self.source.is_symlink(file) {
                if let Ok(target) = self.source.read_link(file) {
                    if dry_run {
                        debug!(
                            "Dry-run: Would create symlink {:?} -> {:?}",
                            dest_file, target
                        );
                    } else if let Err(e) = self.sink.create_symlink(&target, &dest_file) {
                        error!(
                            "Failed to create symlink {:?} -> {:?}: {}",
                            dest_file, target, e
                        );
                    }
                } else {
                    // Handle broken symlinks
                    if dry_run {
                        debug!(
                            "Dry-run: Would create broken symlink {:?} -> {:?}",
                            dest_file, file
                        );
                    } else if let Err(e) = self.sink.create_symlink(file, &dest_file) {
                        error!(
                            "Failed to create broken symlink {:?} -> {:?}: {}",
                            dest_file, file, e
                        );
                    }
                }
            } else {
                // Copy regular file
                if dry_run {
                    debug!("Dry-run: Would copy {:?} to {:?}", file, dest_file);
                } else if let Err(e) = self.sink.copy_file(file, &dest_file) {
                    error!("Failed to copy {:?}: {}", file, e);
                }
            }

            if let Some(pb) = pb {
                pb.inc(*size);
            }
        });
    }

    /// Compare directories and report differences (for local-to-local only)
    pub fn compare_dirs_local(
        source_root: &PathBuf,
        dest_root: &PathBuf,
    ) -> Status {
        let src_files_paths: HashSet<_> = WalkDir::new(source_root)
            .into_iter()
            .filter_map(Result::ok)
            .map(|e| e.path().strip_prefix(source_root).unwrap().to_path_buf())
            .collect();

        let dest_files_paths: HashSet<_> = WalkDir::new(dest_root)
            .into_iter()
            .filter_map(Result::ok)
            .map(|e| e.path().strip_prefix(dest_root).unwrap().to_path_buf())
            .collect();

        let mut status = Status::Passed;

        // Find files that are only in src or dest
        let missing: Vec<_> = src_files_paths.difference(&dest_files_paths).collect();
        let extra: Vec<_> = dest_files_paths.difference(&src_files_paths).collect();
        let common: Vec<_> = src_files_paths.intersection(&dest_files_paths).collect();

        for path in &missing {
            eprintln!("MISSING in dest: {:?}", path);
            status = Status::Failed;
        }

        for path in &extra {
            eprintln!("EXTRA in dest: {:?}", path);
            status = Status::Failed;
        }

        // Compare common files
        let comparison_results: Vec<_> = common
            .par_iter()
            .map(|path| {
                let result = Self::compare_file_metadata_local(source_root, dest_root, path);
                if result == Status::Failed {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();

        status = comparison_results.iter().fold(status, |acc, &_| acc.not());

        status
    }

    /// Compare metadata of a single file (for local-to-local only)
    fn compare_file_metadata_local(
        src_root: &PathBuf,
        dest_root: &PathBuf,
        file: &PathBuf,
    ) -> Status {
        use crate::utils::size_to_human_readable;

        let src_path = src_root.join(file);
        let dest_path = dest_root.join(file);

        let src_meta = fs::symlink_metadata(&src_path).ok();
        let dest_meta = fs::symlink_metadata(&dest_path).ok();

        let mut status = Status::Passed;

        if let (Some(src_meta), Some(dest_meta)) = (src_meta, dest_meta) {
            // Check file size
            if src_meta.len() != dest_meta.len() {
                status = Status::Failed;
                eprintln!(
                    "SIZE MISMATCH: {:?} (src: {}, dest: {})",
                    file,
                    size_to_human_readable(src_meta.len() as f64),
                    size_to_human_readable(dest_meta.len() as f64)
                );
            }

            // Check if both are symlinks or not
            let src_is_symlink = src_meta.file_type().is_symlink();
            let dest_is_symlink = dest_meta.file_type().is_symlink();
            if src_is_symlink != dest_is_symlink {
                status = Status::Failed;
                eprintln!(
                    "TYPE MISMATCH: {:?} (src: {}, dest: {})",
                    file,
                    if src_is_symlink {
                        "symlink"
                    } else {
                        "regular file"
                    },
                    if dest_is_symlink {
                        "symlink"
                    } else {
                        "regular file"
                    }
                );
            }

            // Check file content by comparing Blake3 hashes
            if !src_is_symlink && src_meta.is_file() {
                let src_hash = Self::compute_hash_local(&src_path);
                let dest_hash = Self::compute_hash_local(&dest_path);

                match (src_hash, dest_hash) {
                    (Some(src_h), Some(dest_h)) => {
                        if src_h != dest_h {
                            eprintln!(
                                "CHECKSUM MISMATCH: src: {:?}, src checksum: {}, dest: {:?}, dest checksum: {}",
                                src_path, src_h, dest_path, dest_h
                            );
                            status = Status::Failed;
                        }
                    }
                    _ => {
                        error!(
                            "Hashing failed. for src: {:?}, dest: {:?}",
                            src_path, dest_path
                        );
                        status = Status::Failed;
                    }
                }
            }
        }

        status
    }

    /// Compute hash for a local file
    fn compute_hash_local(path: &PathBuf) -> Option<String> {
        use blake3::Hasher;
        use std::fs::File;
        use std::io::Read;

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
