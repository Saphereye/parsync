use crate::utils::size_to_human_readable;
use crate::{protocols::protocol::Protocol, utils::Status};
use blake3::Hasher;
use log::{debug, error};
use rayon::prelude::*;
use regex::Regex;
use std::{collections::HashSet, fs, path::PathBuf};
use std::{fs::File, io::Read, ops::Not};
use walkdir::WalkDir;

pub struct LocalProtocal;

impl LocalProtocal {}

impl Protocol<PathBuf> for LocalProtocal {
    fn get_file_list(
        source: &PathBuf,
        destination: Option<&PathBuf>,
        include_regex: Option<String>,
        exclude_regex: Option<String>,
        no_verify: bool,
    ) -> Vec<(PathBuf, u64)> {
        let include = include_regex.map(|r| Regex::new(&r).unwrap());
        let exclude = exclude_regex.map(|r| Regex::new(&r).unwrap());

        WalkDir::new(source)
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
                    if !no_verify && is_file {
                        if let Some(dst_root) = destination {
                            if let Ok(relative) = path.strip_prefix(source) {
                                let dst_path = dst_root.join(relative);
                                if dst_path.exists() {
                                    if let (Some(src_hash), Some(dst_hash)) = (
                                        Self::file_checksum(&path.to_path_buf()),
                                        Self::file_checksum(&dst_path),
                                    ) {
                                        if src_hash == dst_hash {
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
                        fs::symlink_metadata(path).map(|m| m.len()).unwrap_or(0)
                    } else {
                        fs::metadata(path).map(|m| m.len()).unwrap_or(0)
                    };

                    Some((path.to_path_buf(), size))
                } else {
                    None
                }
            })
            .collect()
    }

    fn sync_files(
        files: &Vec<(PathBuf, u64)>,
        source: &PathBuf,
        destination: &PathBuf,
        pb: &Option<indicatif::ProgressBar>,
        dry_run: bool,
    ) {
        let src_arc = source.to_path_buf();
        let dest_arc = destination.to_path_buf();

        files.par_iter().for_each(|(file, size)| {
            let rel_path = file.strip_prefix(&src_arc).unwrap();
            let dest_file = dest_arc.join(rel_path);

            // Ensure destination directory exists
            if let Some(parent) = dest_file.parent() {
                if dry_run {
                    debug!("Dry-run: Would create directory {:?}", parent);
                } else if let Err(e) = fs::create_dir_all(parent) {
                    error!("Failed to create directory {:?}: {}", parent, e);
                    return;
                }
            }

            // Handle empty directories
            if size == &0 && file.is_dir() {
                if dry_run {
                    debug!("Dry-run: Would create empty directory {:?}", dest_file);
                } else if let Err(e) = fs::create_dir_all(&dest_file) {
                    error!("Failed to create directory {:?}: {}", dest_file, e);
                } else {
                    debug!("Created directory {:?}", dest_file);
                }
                return;
            }

            // Copy file
            if dry_run {
                debug!("Dry-run: Would copy {:?} to {:?}", file, dest_file);
            } else if file
                .symlink_metadata()
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
            {
                if let Ok(target) = fs::read_link(file) {
                    if dry_run {
                        debug!(
                            "Dry-run: Would create symlink {:?} -> {:?}",
                            dest_file, target
                        );
                    } else if let Err(e) = Self::create_symlink(&target, &dest_file) {
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
                    } else if let Err(e) = Self::create_symlink(file, &dest_file) {
                        error!(
                            "Failed to create broken symlink {:?} -> {:?}: {}",
                            dest_file, file, e
                        );
                    }
                }
            } else if let Err(e) = fs::copy(file, &dest_file) {
                error!("Failed to copy {:?}: {}", file, e);
            }

            if let Some(pb) = pb {
                pb.inc(*size);
            }
        });
    }

    fn compare_dirs(src: &PathBuf, dest: &PathBuf) -> Status {
        let src_files_paths: HashSet<_> = WalkDir::new(src)
            .into_iter()
            .filter_map(Result::ok)
            .map(|e| e.path().strip_prefix(src).unwrap().to_path_buf())
            .collect();

        let dest_files_paths: HashSet<_> = WalkDir::new(dest)
            .into_iter()
            .filter_map(Result::ok)
            .map(|e| e.path().strip_prefix(dest).unwrap().to_path_buf())
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

        let comparison_results: Vec<_> = common
            .par_iter()
            .map(|path| {
                let result = Self::compare_file_metadata(src, dest, path);
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

    fn compare_file_metadata(src: &PathBuf, dest: &PathBuf, file: &PathBuf) -> Status {
        let src_path = src.join(file);
        let dest_path = dest.join(file);

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

            //// Check file permissions (Unix only)
            //#[cfg(unix)]
            //{
            //    if src_meta.mode() != dest_meta.mode() {
            //        status = Status::Failed;
            //        info!(
            //            "PERMISSION MISMATCH: {:?} (src: {:o}, dest: {:o})",
            //            file,
            //            src_meta.mode(),
            //            dest_meta.mode()
            //        );
            //    }
            //}
            //
            //// Check file permissions (Windows only)
            //#[cfg(windows)]
            //{
            //    if src_meta.permissions().readonly() != dest_meta.permissions().readonly() {
            //        status = Status::Failed;
            //        eprintln!(
            //            "READONLY MISMATCH: {:?} (src: {}, dest: {})",
            //            file,
            //            src_meta.permissions().readonly(),
            //            dest_meta.permissions().readonly()
            //        );
            //    }
            //}

            // Check file content by comparing Blake3 hashes
            match (
                Self::file_checksum(&src_path).ok_or(()),
                Self::file_checksum(&dest_path).ok_or(()),
            ) {
                (Ok(src_hash), Ok(dest_hash)) => {
                    if src_hash != dest_hash {
                        eprintln!("CHECKSUM MISMATCH: src: {:?}, src checksum: {}, dest: {:?}, dest checksum: {}", src_path, src_hash, dest_path, dest_hash);
                        status = Status::Failed;
                    }
                }
                (Err(_), Err(_)) | (Err(_), Ok(_)) | (Ok(_), Err(_)) => {
                    error!(
                        "Hashing failed. for src: {:?}, dest: {:?}",
                        src_path, dest_path
                    );
                    status = Status::Failed;
                }
            }
        }

        status
    }

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

    fn create_symlink(target: &PathBuf, link: &PathBuf) -> std::io::Result<()> {
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
}
