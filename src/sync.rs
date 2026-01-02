#![allow(dead_code)]

//! Parallel file synchronization logic for parsync.
//!
//! Current behavior:
//! - Optimized for local filesystems: uses whole-file copy when a file has changed.
//! - Fast skip for unchanged files via size + modified time (mtime) comparison.
//! - Lock-free work sharing: workers pull file jobs via an atomic index (no channels).
//! - Per-worker cache of created directories to reduce redundant create_dir_all calls.
//! - Progress bar shows total bytes processed (skips and copies).
//!
//! Note:
//! - Previous Adler-32 rolling checksum logic was removed to reduce CPU overhead.
//! - For remote or low-bandwidth scenarios, a chunked delta approach can be reintroduced behind a feature flag.

use crate::backends::{StorageBackend, SyncError};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use walkdir::WalkDir;

/// Default chunk size: 1 MiB
pub const DEFAULT_CHUNK_SIZE: usize = 1 << 20;
pub const LARGE_FILE_THRESHOLD: u64 = 32 * 1024 * 1024; // 32 MiB

/// Represents a file to sync.
struct FileJob {
    src_path: PathBuf,
    dst_path: PathBuf,
    size: u64,
    src_modified: Option<std::time::SystemTime>,
}

/// Recursively synchronize a directory from source to destination using parallel workers.
/// - Creates directories at the destination as needed (including empty ones).
/// - Skips unchanged files via size + mtime; performs whole-file copy on change.
/// - Designed for local filesystems; per-worker directory creation cache reduces overhead.
pub fn sync(
    _src_backend: Arc<dyn StorageBackend + Send + Sync>,
    src_root: &str,
    _dst_backend: Arc<dyn StorageBackend + Send + Sync>,
    dst_root: &str,
    _chunk_size: usize,
    no_progress: bool,
) -> Result<(), SyncError> {
    let src_root_path = Path::new(src_root);
    let dst_root_path = Path::new(dst_root);

    // First pass: collect all files and total size
    let mut files = Vec::new();
    let mut total_bytes = 0u64;
    for entry in WalkDir::new(src_root).min_depth(0) {
        let entry = entry.map_err(|e| SyncError::Other(format!("WalkDir error: {e}")))?;
        let src_path = entry.path();
        let rel_path = match src_path.strip_prefix(src_root_path) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let dst_path: PathBuf = dst_root_path.join(rel_path);
        let file_type = entry.file_type();
        if file_type.is_dir() {
            if !dst_path.exists() {
                std::fs::create_dir_all(&dst_path).map_err(|e| {
                    SyncError::Other(format!("Failed to create dir {:?}: {e}", dst_path))
                })?;
            }
        } else if file_type.is_file() {
            let meta = entry.metadata();
            let (size, src_modified) = match meta {
                Ok(m) => (m.len(), m.modified().ok()),
                Err(_) => (0, None),
            };
            total_bytes += size;
            files.push(FileJob {
                src_path: src_path.to_path_buf(),
                dst_path,
                size,
                src_modified,
            });
        }
    }

    // Progress bar setup
    let pb = if no_progress {
        None
    } else {
        let pb = ProgressBar::new(total_bytes);
        pb.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}",
            )
            .unwrap()
            .progress_chars("##-"),
        );
        pb.set_message("Syncing...");
        Some(pb)
    };

    // Parallel work-sharing without channels: atomic index over files; workers update progress directly
    let files = Arc::new(files);
    let index = Arc::new(AtomicUsize::new(0));
    let total_files = files.len();

    let num_threads = num_cpus::get().max(2);
    let mut workers = Vec::new();
    let pb_shared = pb.clone();

    for _ in 0..num_threads {
        let files = Arc::clone(&files);
        let index = Arc::clone(&index);
        let pb_worker = pb_shared.clone();
        workers.push(thread::spawn(move || {
            let mut created_dirs = HashSet::new();
            loop {
                let i = index.fetch_add(1, Ordering::Relaxed);
                if i >= total_files {
                    break;
                }
                let file = &files[i];

                // Quick skip using pre-scanned src modified time + size
                let dst_meta = std::fs::metadata(&file.dst_path).ok();
                let mut skipped = false;
                if let Some(ref dm) = dst_meta {
                    if file.size == dm.len() {
                        if let (Some(st), Ok(dt)) = (file.src_modified, dm.modified()) {
                            if st == dt {
                                if let Some(ref pb) = pb_worker {
                                    pb.inc(file.size);
                                }
                                skipped = true;
                            }
                        }
                    }
                }
                if skipped {
                    continue;
                }

                // Copy whole file for any changed file (fast path)
                if let Some(parent) = file.dst_path.parent() {
                    if !created_dirs.contains(parent) {
                        let _ = std::fs::create_dir_all(parent);
                        created_dirs.insert(parent.to_path_buf());
                    }
                }
                let copied =
                    fast_copy(&file.src_path, &file.dst_path, file.size, file.src_modified);
                if let Some(ref pb) = pb_worker {
                    pb.inc(copied);
                }
            }
        }));
    }

    for w in workers {
        let _ = w.join();
    }
    if let Some(ref pb) = pb {
        pb.finish_with_message("Sync complete");
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn fast_copy(
    src: &Path,
    dst: &Path,
    size: u64,
    src_modified: Option<std::time::SystemTime>,
) -> u64 {
    use std::fs::OpenOptions;
    use std::os::unix::io::AsRawFd;

    let mut copied_bytes: u64 = 0;

    let src_f = match OpenOptions::new().read(true).open(src) {
        Ok(f) => f,
        Err(_) => return 0,
    };
    let dst_f = match OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(dst)
    {
        Ok(f) => f,
        Err(_) => return 0,
    };

    unsafe {
        // Try reflink/clone (FICLONE) first
        const FICLONE: libc::c_ulong = 0x4004_9409;
        if libc::ioctl(dst_f.as_raw_fd(), FICLONE, src_f.as_raw_fd()) == 0 {
            copied_bytes = size;
        } else {
            // Try copy_file_range loop
            let in_fd = src_f.as_raw_fd();
            let out_fd = dst_f.as_raw_fd();
            let mut off_in: libc::loff_t = 0;
            let mut off_out: libc::loff_t = 0;
            loop {
                let n = libc::copy_file_range(in_fd, &mut off_in, out_fd, &mut off_out, 1 << 30, 0);
                if n <= 0 {
                    break;
                }
                copied_bytes = copied_bytes.saturating_add(n as u64);
                if copied_bytes >= size {
                    break;
                }
            }

            // Fallback to sendfile if nothing copied
            if copied_bytes == 0 {
                let mut offset: libc::off_t = 0;
                loop {
                    let n = libc::sendfile(out_fd, in_fd, &mut offset, 1 << 30);
                    if n <= 0 {
                        break;
                    }
                    copied_bytes = copied_bytes.saturating_add(n as u64);
                    if copied_bytes >= size {
                        break;
                    }
                }
            }
        }
    }

    if copied_bytes == 0 {
        copied_bytes = std::fs::copy(src, dst).unwrap_or(0);
    }

    if let Some(st) = src_modified {
        let _ = filetime::set_file_mtime(dst, filetime::FileTime::from_system_time(st));
    }

    copied_bytes
}

#[cfg(not(target_os = "linux"))]
fn fast_copy(
    src: &Path,
    dst: &Path,
    _size: u64,
    src_modified: Option<std::time::SystemTime>,
) -> u64 {
    let copied = std::fs::copy(src, dst).unwrap_or(0);
    if let Some(st) = src_modified {
        let _ = filetime::set_file_mtime(dst, filetime::FileTime::from_system_time(st));
    }
    copied
}

// write_chunk removed; writes are performed using the already opened dst_file handle
