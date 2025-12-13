pub mod backends;

pub use backends::{FileEntry, LocalBackend, StorageBackend, SyncError};

use crossbeam_channel::unbounded;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
/// Copy a file or directory from source to destination using the provided backends.
use walkdir::WalkDir;

/// Progress trait for atomic-free progress bar abstraction
trait Progress: Send + Sync {
    fn inc(&self, n: u64);
    fn inc_length(&self, n: u64);
    fn finish_with_message(&self, msg: &'static str);
}

impl Progress for ProgressBar {
    fn inc(&self, n: u64) {
        ProgressBar::inc(self, n)
    }
    fn inc_length(&self, n: u64) {
        ProgressBar::inc_length(self, n)
    }
    fn finish_with_message(&self, msg: &'static str) {
        ProgressBar::finish_with_message(self, msg)
    }
}

struct NoProgress;
impl Progress for NoProgress {
    fn inc(&self, _n: u64) {}
    fn inc_length(&self, _n: u64) {}
    fn finish_with_message(&self, _msg: &'static str) {}
}

pub struct CopyOptions<'a> {
    pub threads: usize,
    pub include: Option<&'a regex::Regex>,
    pub exclude: Option<&'a regex::Regex>,
    pub dry_run: bool,
    pub no_progress: bool,
}

pub fn copy(
    source: Arc<dyn crate::backends::StorageBackend + Sync + Send>,
    source_path: &str,
    dest: Arc<dyn crate::backends::StorageBackend + Sync + Send>,
    dest_path: &str,
    options: &CopyOptions,
) -> Result<(), SyncError> {
    // Channel for file paths
    let (tx, rx) = unbounded();

    // Progress bar: dynamic length, starts at 0 and grows as bytes are discovered
    let pb: Arc<Box<dyn Progress>> = if options.no_progress {
        Arc::new(Box::new(NoProgress))
    } else {
        let pb = ProgressBar::new(0);
        pb.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
            )
            .unwrap()
            .progress_chars("##-"),
        );
        Arc::new(Box::new(pb))
    };

    // Producer thread: walks the directory and sends file paths
    let source_path_buf = source_path.to_string();
    let include = options.include.cloned();
    let exclude = options.exclude.cloned();
    let tx_producer = tx.clone();
    let pb_producer = pb.clone();
    let producer = thread::spawn(move || {
        for entry in WalkDir::new(&source_path_buf)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let file_str = entry.path().to_string_lossy().to_string();
            if let Some(ref re) = include {
                if !re.is_match(&file_str) {
                    continue;
                }
            }
            if let Some(ref re) = exclude {
                if re.is_match(&file_str) {
                    continue;
                }
            }
            let rel_path = entry
                .path()
                .strip_prefix(&source_path_buf)
                .unwrap()
                .to_path_buf();
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            tx_producer
                .send((rel_path, size))
                .expect("Failed to send file path and size");
            pb_producer.inc_length(size);
        }
        // Drop the sender to close the channel
        drop(tx_producer);
    });
    // Drop the original sender in the main thread to ensure channel closes when producer is done
    drop(tx);

    // Worker threads: receive file paths and copy files
    let mut handles = Vec::new();
    let rx = Arc::new(rx);
    let errors: Arc<Mutex<Vec<SyncError>>> = Arc::new(Mutex::new(Vec::new()));

    for _ in 0..options.threads {
        let rx = Arc::clone(&rx);
        let source = Arc::clone(&source);
        let dest = Arc::clone(&dest);
        let pb = pb.clone();
        let dest_path = dest_path.to_string();
        let source_path = source_path.to_string();
        let dry_run = options.dry_run;
        let errors = Arc::clone(&errors);

        let handle = thread::spawn(move || {
            // Allocate one buffer per worker thread for streaming copy
            let mut buf = vec![0u8; 1024 * 1024]; // 1 MiB buffer
            while let Ok((rel_path, size)) = rx.recv() {
                // Avoid repeated allocations and conversions
                let src_file = Path::new(&source_path).join(&rel_path);
                let dst_file = Path::new(&dest_path).join(&rel_path);

                if dry_run {
                    pb.inc(size);
                    continue;
                }

                let is_local_src = source
                    .as_ref()
                    .as_any()
                    .is::<crate::backends::LocalBackend>();
                let is_local_dst = dest.as_ref().as_any().is::<crate::backends::LocalBackend>();
                if is_local_src && is_local_dst {
                    if let Some(parent) = dst_file.parent() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            errors.lock().unwrap().push(SyncError::Io(e));
                            pb.inc(size);
                            continue;
                        }
                    }
                    let src_backend = source
                        .as_ref()
                        .as_any()
                        .downcast_ref::<crate::backends::LocalBackend>()
                        .unwrap();
                    match src_backend.copy_file(
                        src_file.to_str().unwrap(),
                        dst_file.to_str().unwrap(),
                        &mut buf,
                    ) {
                        Ok(_) => {}
                        Err(e) => {
                            errors.lock().unwrap().push(e);
                        }
                    }
                } else {
                    // Fallback: get/put
                    match source.get(src_file.to_str().unwrap()) {
                        Ok(data) => {
                            if let Some(parent) = dst_file.parent() {
                                if let Err(e) = std::fs::create_dir_all(parent) {
                                    errors.lock().unwrap().push(SyncError::Io(e));
                                    pb.inc(size);
                                    continue;
                                }
                            }
                            match dest.put(dst_file.to_str().unwrap(), &data) {
                                Ok(_) => {}
                                Err(e) => {
                                    errors.lock().unwrap().push(e);
                                }
                            }
                        }
                        Err(e) => {
                            errors.lock().unwrap().push(e);
                        }
                    }
                }
                pb.inc(size);
            }
            log::info!("Worker exiting");
        });
        handles.push(handle);
    }

    // Wait for producer and workers
    producer.join().expect("Producer thread panicked");
    for (i, handle) in handles.into_iter().enumerate() {
        handle.join().expect("Worker thread panicked");
        log::info!("Joined worker thread {}", i);
    }
    pb.finish_with_message("Copy complete");

    let errors = Arc::try_unwrap(errors).unwrap().into_inner().unwrap();
    if !errors.is_empty() {
        return Err(SyncError::Other(format!(
            "{} errors occurred during copy",
            errors.len()
        )));
    }
    Ok(())
}
