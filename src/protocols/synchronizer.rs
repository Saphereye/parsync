use crate::protocols::sink::Sink;
use crate::protocols::source::Source;
use indicatif::ProgressBar;
use log::{debug, error};
use rayon::prelude::*;
use regex::Regex;
use std::path::PathBuf;
use walkdir::WalkDir;

/// Synchronizer that works with any Source and Sink implementation
pub struct Synchronizer<S: Source, D: Sink> {
    source: S,
    sink: D,
}

impl<S: Source, D: Sink> Synchronizer<S, D> {
    pub fn new(source: S, sink: D) -> Self {
        Self { source, sink }
    }

    /// Get list of files that need to be synced
    /// This implements the new hash comparison logic:
    /// 1. Get list of files from source
    /// 2. For each file, check if it exists at destination
    /// 3. If it exists, get hash from destination
    /// 4. Compare hash at source
    /// 5. Only include files that are missing or have different hashes
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

    /// Sync files from source to sink
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
}
