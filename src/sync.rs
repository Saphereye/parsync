#![allow(dead_code)]

//! Parallel chunked file synchronization logic for parsync.
//!
//! Uses Adler-32 rolling checksums for chunk comparison and only copies changed chunks.
//! Designed for the case where most contents match (rsync-like).
//! No cryptographic hash is used for verification (for now).

use crate::backends::{StorageBackend, SyncError};
use adler::Adler32;
use crossbeam_channel::unbounded;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use walkdir::WalkDir;

/// Default chunk size: 1 MiB
pub const DEFAULT_CHUNK_SIZE: usize = 1 << 20;
pub const LARGE_FILE_THRESHOLD: u64 = 32 * 1024 * 1024; // 32 MiB

/// Represents a chunk job for a file.
struct ChunkJob {
    src_path: PathBuf,
    dst_path: PathBuf,
    chunk_index: usize,
    offset: u64,
    size: usize,
}

/// Represents a file to sync.
struct FileJob {
    src_path: PathBuf,
    dst_path: PathBuf,
    size: u64,
}

/// Recursively synchronize a directory from source to destination using parallel chunked Adler-32 checksums.
/// - Creates directories at the destination as needed (including empty ones).
/// - Syncs files using chunked sync.
/// - Skips symlinks and special files.
pub fn sync_dir_chunked(
    _src_backend: Arc<dyn StorageBackend + Send + Sync>,
    src_root: &str,
    _dst_backend: Arc<dyn StorageBackend + Send + Sync>,
    dst_root: &str,
    chunk_size: usize,
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
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            total_bytes += size;
            files.push(FileJob {
                src_path: src_path.to_path_buf(),
                dst_path,
                size,
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

    // Producer-consumer model for parallel chunked sync
    let (job_tx, job_rx) = unbounded();
    let (done_tx, done_rx) = unbounded();

    // Clone done_tx for producer and workers before moving into threads
    let producer_done_tx = done_tx.clone();

    // Producer: walk files, enqueue chunk jobs or copy small files directly
    let producer = {
        let job_tx = job_tx.clone();
        thread::spawn(move || {
            for file in files {
                // Metadata shortcut: skip hashing/copy if size and mtime match
                let src_meta = match std::fs::metadata(&file.src_path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let dst_meta = std::fs::metadata(&file.dst_path).ok();

                let mut skip = false;
                if let Some(ref dst_meta) = dst_meta {
                    if src_meta.len() == dst_meta.len() {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::MetadataExt;
                            if src_meta.mtime() == dst_meta.mtime()
                                && src_meta.mtime_nsec() == dst_meta.mtime_nsec()
                            {
                                skip = true;
                            }
                        }
                        #[cfg(not(unix))]
                        {
                            use std::time::SystemTime;
                            if let (Ok(src_time), Ok(dst_time)) =
                                (src_meta.modified(), dst_meta.modified())
                            {
                                if src_time == dst_time {
                                    skip = true;
                                }
                            }
                        }
                    }
                }

                if skip {
                    // File is unchanged, just send a "done" for progress
                    let _ = producer_done_tx.send(file.size);
                    continue;
                }

                // If destination doesn't exist or file is small, copy whole file
                if dst_meta.is_none() || file.size < LARGE_FILE_THRESHOLD {
                    match std::fs::copy(&file.src_path, &file.dst_path) {
                        Ok(copied) => {
                            let _ = producer_done_tx.send(copied);
                        }
                        Err(_) => {
                            // If copy fails, just send 0
                            let _ = producer_done_tx.send(0);
                        }
                    }
                    continue;
                }

                // Otherwise, enqueue chunk jobs for parallel comparison/copy
                let num_chunks = file.size.div_ceil(chunk_size as u64);
                for chunk_index in 0..num_chunks {
                    let offset = chunk_index * chunk_size as u64;
                    let size = if offset + chunk_size as u64 > file.size {
                        (file.size - offset) as usize
                    } else {
                        chunk_size
                    };
                    let job = ChunkJob {
                        src_path: file.src_path.clone(),
                        dst_path: file.dst_path.clone(),
                        chunk_index: chunk_index.try_into().unwrap(),
                        offset,
                        size,
                    };
                    let _ = job_tx.send(job);
                }
            }
            // Drop sender to signal end of jobs
            drop(job_tx);
            drop(producer_done_tx);
        })
    };

    // Worker threads: compare/copy chunks in parallel
    let num_threads = num_cpus::get().max(2);
    let mut workers = Vec::new();
    for _ in 0..num_threads {
        let job_rx = job_rx.clone();
        let worker_done_tx = done_tx.clone();
        workers.push(thread::spawn(move || {
            for job in job_rx.iter() {
                // Read chunk from source
                let mut src_file = match File::open(&job.src_path) {
                    Ok(f) => f,
                    Err(_) => {
                        let _ = worker_done_tx.send(0);
                        continue;
                    }
                };
                let mut src_buf = vec![0u8; job.size];
                if src_file.seek(SeekFrom::Start(job.offset)).is_err() {
                    let _ = worker_done_tx.send(0);
                    continue;
                }
                let n = match src_file.read(&mut src_buf) {
                    Ok(n) => n,
                    Err(_) => {
                        let _ = worker_done_tx.send(0);
                        continue;
                    }
                };
                if n == 0 {
                    let _ = worker_done_tx.send(0);
                    continue;
                }

                // Compute Adler-32 of source chunk
                let mut src_adler = Adler32::new();
                src_adler.write_slice(&src_buf[..n]);
                let src_sum = src_adler.checksum();

                // Try to read chunk from destination
                let mut dst_file = match OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&job.dst_path)
                {
                    Ok(f) => f,
                    Err(_) => {
                        // If destination can't be opened, treat as changed
                        let _ = write_chunk(&job.dst_path, job.offset, &src_buf[..n]);
                        let _ = worker_done_tx.send(n as u64);
                        continue;
                    }
                };
                let mut dst_buf = vec![0u8; n];
                if dst_file.seek(SeekFrom::Start(job.offset)).is_err() {
                    let _ = write_chunk(&job.dst_path, job.offset, &src_buf[..n]);
                    let _ = worker_done_tx.send(n as u64);
                    continue;
                }
                let m = dst_file.read(&mut dst_buf).unwrap_or_default();

                // Compute Adler-32 of destination chunk
                let mut dst_adler = Adler32::new();
                dst_adler.write_slice(&dst_buf[..m]);
                let dst_sum = dst_adler.checksum();

                if n != m || src_sum != dst_sum {
                    // Chunks differ, write source chunk to destination
                    let _ = write_chunk(&job.dst_path, job.offset, &src_buf[..n]);
                    let _ = worker_done_tx.send(n as u64);
                } else {
                    // Chunks match, just update progress
                    let _ = worker_done_tx.send(n as u64);
                }
            }
        }));
    }

    // Progress bar updater
    let pb_thread = {
        let pb = pb.clone();
        thread::spawn(move || {
            for n in done_rx.iter() {
                if let Some(ref pb) = pb {
                    pb.inc(n);
                }
            }
            if let Some(ref pb) = pb {
                pb.finish_with_message("Sync complete");
            }
        })
    };

    // Wait for producer and workers to finish
    let _ = producer.join();
    for w in workers {
        let _ = w.join();
    }
    let _ = pb_thread.join();

    Ok(())
}

fn write_chunk(dst_path: &Path, offset: u64, buf: &[u8]) -> std::io::Result<()> {
    let mut dst_file = OpenOptions::new().write(true).open(dst_path)?;
    dst_file.seek(SeekFrom::Start(offset))?;
    dst_file.write_all(buf)?;
    Ok(())
}
