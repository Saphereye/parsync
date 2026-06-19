pub mod backends;
pub mod sync;
pub mod utils;

pub use backends::{
    backend_and_path, FileEntry, LocalBackend, SshBackend, StorageBackend, SyncError,
};
pub use sync::sync;

use crossbeam_channel::unbounded;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use walkdir::WalkDir;

pub struct CopyOptions<'a> {
    pub threads: usize,
    pub include: Option<&'a regex::Regex>,
    pub exclude: Option<&'a regex::Regex>,
    pub dry_run: bool,
    pub no_progress: bool,
    pub no_preserve_times: bool,
}

pub fn copy(
    source: Arc<dyn crate::backends::StorageBackend + Sync + Send>,
    source_path: &str,
    dest: Arc<dyn crate::backends::StorageBackend + Sync + Send>,
    dest_path: &str,
    options: &CopyOptions,
) -> Result<(), SyncError> {
    let (tx, rx) = unbounded();

    let pb = if options.no_progress {
        None
    } else {
        let pb = ProgressBar::new_spinner();
        pb.set_message("Queuing files...");
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        Some(pb)
    };

    let source_path_buf = source_path.to_string();
    let include = options.include.cloned();
    let exclude = options.exclude.cloned();
    let tx_producer = tx.clone();
    let pb_producer = pb.clone();
    let producer = thread::spawn(move || {
        let mut total_bytes = 0u64;
        let mut file_count = 0u64;
        let mut switched = false;
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
            total_bytes += size;
            file_count += 1;
            if let Some(pb) = pb_producer.as_ref() {
                if !switched && file_count == 1 {
                    pb.set_style(
                        ProgressStyle::with_template(
                            "[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
                        )
                        .unwrap()
                        .progress_chars("##-"),
                    );
                    pb.set_length(0);
                    pb.set_message("Copying...");
                    switched = true;
                }
                pb.set_length(total_bytes);
            }
        }
        drop(tx_producer);
    });
    drop(tx);

    let is_local_src = source
        .as_ref()
        .as_any()
        .is::<crate::backends::LocalBackend>();
    let is_local_dst = dest.as_ref().as_any().is::<crate::backends::LocalBackend>();
    let both_local = is_local_src && is_local_dst;

    let mut handles = Vec::new();
    let rx = Arc::new(rx);
    let errors: Arc<Mutex<Vec<SyncError>>> = Arc::new(Mutex::new(Vec::new()));

    for _ in 0..options.threads {
        let rx = Arc::clone(&rx);
        let source = Arc::clone(&source);
        let dest = Arc::clone(&dest);
        let pb_worker = pb.clone();
        let dest_path = dest_path.to_string();
        let source_path = source_path.to_string();
        let dry_run = options.dry_run;
        let no_preserve_times = options.no_preserve_times;
        let errors = Arc::clone(&errors);

        let handle = thread::spawn(move || {
            let mut src_file = PathBuf::with_capacity(256);
            let mut dst_file = PathBuf::with_capacity(256);
            let mut _buf = vec![0u8; 1024 * 1024];
            let mut created_dirs: std::collections::HashSet<PathBuf> =
                std::collections::HashSet::new();

            while let Ok((rel_path, size)) = rx.recv() {
                src_file.clear();
                src_file.push(&source_path);
                src_file.push(&rel_path);

                dst_file.clear();
                dst_file.push(&dest_path);
                dst_file.push(&rel_path);

                if dry_run {
                    if let Some(pb) = pb_worker.as_ref() {
                        pb.inc(size);
                    }
                    continue;
                }

                if both_local {
                    if let Some(parent) = dst_file.parent() {
                        if !created_dirs.contains(parent) {
                            if let Err(e) = std::fs::create_dir_all(parent) {
                                errors.lock().unwrap().push(SyncError::Io(e));
                                if let Some(pb) = pb_worker.as_ref() {
                                    pb.inc(size);
                                }
                                continue;
                            }
                            created_dirs.insert(parent.to_path_buf());
                        }
                    }

                    let mut copied = fast_copy(
                        &src_file,
                        &dst_file,
                        size,
                        std::fs::metadata(src_file.to_str().unwrap())
                            .ok()
                            .and_then(|m| m.modified().ok()),
                        !no_preserve_times,
                    );
                    if copied == 0 {
                        match std::fs::copy(src_file.to_str().unwrap(), dst_file.to_str().unwrap())
                        {
                            Ok(n) => {
                                copied = n;
                                if !no_preserve_times {
                                    if let Ok(src_meta) =
                                        std::fs::metadata(src_file.to_str().unwrap())
                                    {
                                        if let Ok(st) = src_meta.modified() {
                                            let _ = filetime::set_file_mtime(
                                                dst_file.to_str().unwrap(),
                                                filetime::FileTime::from_system_time(st),
                                            );
                                        }
                                    }
                                }
                            }
                            Err(fs_err) => {
                                if let Some(src_local) = source
                                    .as_ref()
                                    .as_any()
                                    .downcast_ref::<crate::backends::LocalBackend>(
                                ) {
                                    match src_local.copy_file(
                                        src_file.to_str().unwrap(),
                                        dst_file.to_str().unwrap(),
                                        &mut _buf,
                                    ) {
                                        Ok(_) => {
                                            copied = size;
                                        }
                                        Err(be_err) => {
                                            errors
                                                .lock()
                                                .unwrap()
                                                .push(SyncError::Other(format!(
                                                    "Failed to copy {:?}: std fs error: {:?}, backend error: {:?}",
                                                    src_file, fs_err, be_err
                                                )));
                                            if let Some(pb) = pb_worker.as_ref() {
                                                pb.inc(size);
                                            }
                                            continue;
                                        }
                                    }
                                } else {
                                    errors
                                        .lock()
                                        .unwrap()
                                        .push(SyncError::Other(format!(
                                            "Failed to copy {:?}: std fs error: {:?}, no LocalBackend fallback",
                                            src_file, fs_err
                                        )));
                                    if let Some(pb) = pb_worker.as_ref() {
                                        pb.inc(size);
                                    }
                                    continue;
                                }
                            }
                        }
                    }
                    if let Some(pb) = pb_worker.as_ref() {
                        pb.inc(copied.max(size));
                    }
                    continue;
                } else if is_local_src {
                    if is_local_dst {
                        if let Some(parent) = dst_file.parent() {
                            if let Err(e) = std::fs::create_dir_all(parent) {
                                errors.lock().unwrap().push(SyncError::Io(e));
                                if let Some(pb) = pb_worker.as_ref() {
                                    pb.inc(size);
                                }
                                continue;
                            }
                        }
                    }
                    match std::fs::File::open(&src_file) {
                        Ok(mut f) => {
                            if let Err(e) =
                                dest.put_stream(dst_file.to_str().unwrap(), &mut f, size)
                            {
                                errors.lock().unwrap().push(e);
                            }
                        }
                        Err(e) => errors.lock().unwrap().push(SyncError::Io(e)),
                    }
                    if let Some(pb) = pb_worker.as_ref() {
                        pb.inc(size);
                    }
                } else {
                    if is_local_dst {
                        if let Some(parent) = dst_file.parent() {
                            if let Err(e) = std::fs::create_dir_all(parent) {
                                errors.lock().unwrap().push(SyncError::Io(e));
                                if let Some(pb) = pb_worker.as_ref() {
                                    pb.inc(size);
                                }
                                continue;
                            }
                        }
                    }
                    match source.get(src_file.to_str().unwrap()) {
                        Ok(data) => {
                            if let Err(e) = dest.put(dst_file.to_str().unwrap(), &data) {
                                errors.lock().unwrap().push(e);
                            }
                        }
                        Err(e) => errors.lock().unwrap().push(e),
                    }
                    if let Some(pb) = pb_worker.as_ref() {
                        pb.inc(size);
                    }
                }
            }
        });
        handles.push(handle);
    }

    producer.join().expect("Producer thread panicked");
    for handle in handles {
        handle.join().expect("Worker thread panicked");
    }
    if let Some(pb) = pb.as_ref() {
        pb.finish_with_message("Copy complete");
    }

    let errors = Arc::try_unwrap(errors).unwrap().into_inner().unwrap();
    if !errors.is_empty() {
        return Err(SyncError::Other(format!(
            "{} errors occurred during copy",
            errors.len()
        )));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn fast_copy(
    src: &std::path::Path,
    dst: &std::path::Path,
    size: u64,
    src_modified: Option<std::time::SystemTime>,
    preserve_times: bool,
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
        const FICLONE: libc::c_ulong = 0x4004_9409;
        if libc::ioctl(dst_f.as_raw_fd(), FICLONE, src_f.as_raw_fd()) == 0 {
            copied_bytes = size;
        } else {
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

    if preserve_times {
        if let Some(st) = src_modified {
            let _ = filetime::set_file_mtime(dst, filetime::FileTime::from_system_time(st));
        }
    }

    copied_bytes
}

#[cfg(not(target_os = "linux"))]
fn fast_copy(
    src: &std::path::Path,
    dst: &std::path::Path,
    _size: u64,
    src_modified: Option<std::time::SystemTime>,
    preserve_times: bool,
) -> u64 {
    let copied = std::fs::copy(src, dst).unwrap_or(0);
    if preserve_times {
        if let Some(st) = src_modified {
            let _ = filetime::set_file_mtime(dst, filetime::FileTime::from_system_time(st));
        }
    }
    copied
}

pub fn delete(
    backend: Arc<dyn crate::backends::StorageBackend + Sync + Send>,
    roots: &[String],
    threads: usize,
    dry_run: bool,
    no_progress: bool,
    include: Option<&regex::Regex>,
    exclude: Option<&regex::Regex>,
) -> Result<(), SyncError> {
    use indicatif::{ProgressBar, ProgressStyle};

    let (tx, rx) = crossbeam_channel::unbounded::<PathBuf>();
    let include_producer = include.cloned();
    let exclude_producer = exclude.cloned();

    let pb = if no_progress {
        None
    } else {
        let pb = ProgressBar::new(0);
        pb.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} deleted ({eta})",
            )
            .unwrap()
            .progress_chars("##-"),
        );
        Some(pb)
    };

    let mut files: Vec<PathBuf> = Vec::new();
    let mut dirs: Vec<PathBuf> = Vec::new();

    for root in roots {
        for entry in walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let file_str = entry.path().to_string_lossy();
            if let Some(ref re) = include_producer {
                if !re.is_match(&file_str) {
                    continue;
                }
            }
            if let Some(ref re) = exclude_producer {
                if re.is_match(&file_str) {
                    continue;
                }
            }
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    files.push(entry.path().to_path_buf());
                } else if meta.is_dir() {
                    dirs.push(entry.path().to_path_buf());
                }
            }
        }
    }

    dirs.sort_by_key(|b| std::cmp::Reverse(b.components().count()));

    if let Some(ref pb) = pb {
        pb.set_length((files.len() + dirs.len()) as u64);
    }

    let tx_producer = tx.clone();
    for f in files {
        tx_producer.send(f).expect("send file");
    }
    drop(tx_producer);
    drop(tx);

    let error_acc: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    thread::scope(|s| {
        let rx = Arc::new(rx);
        for _ in 0..threads {
            let rx = Arc::clone(&rx);
            let backend = Arc::clone(&backend);
            let errors = Arc::clone(&error_acc);
            let pb = pb.clone();

            s.spawn(move || {
                while let Ok(path) = rx.recv() {
                    if dry_run {
                        println!("Would delete: {}", path.display());
                        continue;
                    }
                    match backend.delete(&path.to_string_lossy()) {
                        Ok(_) => {
                            if let Some(ref pb) = pb {
                                pb.inc(1);
                            }
                        }
                        Err(e) => errors.lock().unwrap().push(format!("{e:?}")),
                    }
                }
            });
        }
    });

    for dir in &dirs {
        if dry_run {
            println!("Would delete: {}", dir.display());
            continue;
        }
        match backend.delete(&dir.to_string_lossy()) {
            Ok(_) => {
                if let Some(ref pb) = pb {
                    pb.inc(1);
                }
            }
            Err(e) => error_acc.lock().unwrap().push(format!("{e:?}")),
        }
    }

    if let Some(pb) = pb {
        pb.finish_with_message("Delete complete");
    }

    let errors = Arc::try_unwrap(error_acc).unwrap().into_inner().unwrap();
    if !errors.is_empty() {
        return Err(SyncError::Other(format!(
            "{} errors occurred during delete",
            errors.len()
        )));
    }

    Ok(())
}
