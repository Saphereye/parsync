use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, error};
use rayon::prelude::*;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Read;
use std::num::NonZeroUsize;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Get list of all files with their sizes
fn get_file_list(
    source: &Path,
    include_regex: Option<String>,
    exclude_regex: Option<String>,
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

            let is_file_or_symlink = e.file_type().is_file() || e.file_type().is_symlink();
            let is_empty_dir = e.file_type().is_dir()
                && path
                    .read_dir()
                    .map(|mut i| i.next().is_none())
                    .unwrap_or(false);

            if (is_file_or_symlink || is_empty_dir)
                && include
                    .as_ref()
                    .map(|r| r.is_match(&path_str))
                    .unwrap_or(true)
                && !exclude
                    .as_ref()
                    .map(|r| r.is_match(&path_str))
                    .unwrap_or(false)
            {
                let size = if e.file_type().is_dir() {
                    0
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

/// Compute SHA-256 hash of a file (optional integrity check)
fn file_checksum(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];

    while let Ok(n) = file.read(&mut buffer) {
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

/// Copy files in parallel, considering size-based progress
fn sync_files(
    files: &[(PathBuf, u64)],
    source: &Path,
    destination: &Path,
    pb: &Option<ProgressBar>,
    no_verify: bool,
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

        // Check if file needs copying
        if no_verify && dest_file.exists() {
            let src_hash = file_checksum(file);
            let dest_hash = file_checksum(&dest_file);
            if src_hash == dest_hash {
                if let Some(pb) = pb {
                    pb.inc(*size);
                }
                debug!("Skipping {:?}, checksums match", file);
                return;
            } else {
                debug!(
                    "Checksums do not match for {:?}: {:?} != {:?}",
                    file, src_hash, dest_hash
                );
            }
        }

        // Copy file
        if dry_run {
            debug!("Dry-run: Would copy {:?} to {:?}", file, dest_file);
        } else {
            if file.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false) {
                if let Ok(target) = fs::read_link(file) {
                    if !target.exists() {
                        debug!("Broken symlink detected: {:?} -> {:?}. Creating empty file.", file, target);
                        if let Err(e) = File::create(&dest_file) {
                            error!("Failed to create file for broken symlink {:?}: {}", dest_file, e);
                        }
                    } else if let Err(e) = fs::copy(file, &dest_file) {
                        error!("Failed to copy {:?}: {}", file, e);
                    }
                }
            } else if let Err(e) = fs::copy(file, &dest_file) {
                error!("Failed to copy {:?}: {}", file, e);
            }
        } 

        if let Some(pb) = pb {
            pb.inc(*size);
        }
    });
}

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Source directory
    #[arg(short, long, value_name = "SOURCE")]
    source: String,

    /// Destination directory
    #[arg(short, long, value_name = "DESTINATION")]
    destination: String,

    /// Number of threads to use
    #[arg(short, long, value_name = "THREADS", default_value_t = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or_else(|_| NonZeroUsize::new(1).unwrap().get()))]
    threads: usize,

    /// Disables checksum verification
    #[arg(long)]
    no_verify: bool,

    /// Enables verbose output
    #[arg(long)]
    verbose: bool,

    /// Regex for files/folders to include
    #[arg(short, long, value_name = "INCLUDE")]
    include: Option<String>,

    /// Regex for files/folders to exclude
    #[arg(short, long, value_name = "EXCLUDE")]
    exclude: Option<String>,

    /// Enables dry-run mode
    #[arg(long)]
    dry_run: bool,

    /// Enables diffing of source and destination directories
    #[arg(long)]
    diff: bool,
}

fn u64_to_human_readable(size: u64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let mut size = size as f64;
    let mut unit = 0;
    while size >= 1024.0 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.2} {}", size, units[unit])
}
fn strip_top_level(path: &Path) -> PathBuf {
    path.iter().skip(1).collect()
}
fn compare_dirs(src: &Path, dest: &Path) -> bool {
    let mut src_files: Vec<_> = WalkDir::new(src)
        .into_iter()
        .filter_map(Result::ok)
        .map(|e| e.path().strip_prefix(src).unwrap().to_path_buf()) // Strip src itself
        //.map(|p| strip_top_level(&p)) // Strip the first component
        .collect();

    let mut dest_files: Vec<_> = WalkDir::new(dest)
        .into_iter()
        .filter_map(Result::ok)
        .map(|e| e.path().strip_prefix(dest).unwrap().to_path_buf()) // Strip dest itself
        //.map(|p| strip_top_level(&p)) // Strip the first component
        .collect();

    // Sort both lists for linear traversal
    src_files.sort();
    dest_files.sort();

    let mut success = true;
    let (mut i, mut j) = (0, 0);

    while i < src_files.len() || j < dest_files.len() {
        match (src_files.get(i), dest_files.get(j)) {
            (Some(src_file), Some(dest_file)) => match src_file.cmp(dest_file) {
                std::cmp::Ordering::Less => {
                    eprintln!("MISSING in dest: {:?}", src_file);
                    i += 1;
                    success = false;
                }
                std::cmp::Ordering::Greater => {
                    eprintln!("EXTRA in dest: {:?}", dest_file);
                    j += 1;
                    success = false;
                }
                std::cmp::Ordering::Equal => {
                    compare_file_metadata(src, dest, src_file);
                    i += 1;
                    j += 1;
                }
            },
            (Some(src_file), None) => {
                eprintln!("MISSING in dest: {:?}", src_file);
                i += 1;
                success = false;
            }
            (None, Some(dest_file)) => {
                eprintln!("EXTRA in dest: {:?}", dest_file);
                j += 1;
                success = false;
            }
            (None, None) => break,
        }
    }

    success
}

fn compare_file_metadata(src: &Path, dest: &Path, file: &Path) {
    let src_path = src.join(file);
    let dest_path = dest.join(file);

    let src_meta = fs::symlink_metadata(&src_path).ok();
    let dest_meta = fs::symlink_metadata(&dest_path).ok();

    if let (Some(src_meta), Some(dest_meta)) = (src_meta, dest_meta) {
        // Check file size
        if src_meta.len() != dest_meta.len() {
            eprintln!(
                "SIZE MISMATCH: {:?} (src: {} bytes, dest: {} bytes)",
                file,
                src_meta.len(),
                dest_meta.len()
            );
        }

        // Check if both are symlinks or not
        let src_is_symlink = src_meta.file_type().is_symlink();
        let dest_is_symlink = dest_meta.file_type().is_symlink();
        if src_is_symlink != dest_is_symlink {
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

        // Check file permissions (Unix only)
        #[cfg(unix)]
        {
            if src_meta.mode() != dest_meta.mode() {
                eprintln!(
                    "PERMISSION MISMATCH: {:?} (src: {:o}, dest: {:o})",
                    file,
                    src_meta.mode(),
                    dest_meta.mode()
                );
            }
        }
    }
}

fn main() {
    let args = Args::parse();

    let mut builder = env_logger::Builder::new();

    builder.filter_level(log::LevelFilter::Error); // Show errors by default

    if args.verbose {
        builder.filter_level(log::LevelFilter::Debug); // Enable debug logs if verbose is set
    }

    builder.init();

    let source = PathBuf::from(args.source);
    debug!("Set source as: {:?}", source);
    let destination = PathBuf::from(args.destination);
    debug!("Set destination as: {:?}", destination);

    if args.diff {
        let success = compare_dirs(&source, &destination);
        if success {
            println!("Directories are identical");
        } else {
            println!("Directories are different");
        }
        return;
    }

    let mut files = get_file_list(&source, args.include, args.exclude);
    let total_size: u64 = files.iter().map(|(_, size)| *size).sum();
    debug!(
        "Total size relevant files: {}",
        u64_to_human_readable(total_size)
    );
    let num_threads = args.threads;
    debug!("Using {} threads for the process", num_threads);

    // Sort files by size (largest first) for better distribution
    debug!("Sorting file by sizes");
    files.sort_by(|a, b| b.1.cmp(&a.1));

    // Distribute files across threads by balancing total size
    debug!("Calculating the data chunks");
    let mut chunks = vec![vec![]; num_threads];
    let mut chunk_sizes = vec![0; num_threads];
    for (file, size) in files {
        let min_index = chunk_sizes
            .iter()
            .enumerate()
            .min_by_key(|&(_, &size)| size)
            .map(|(index, _)| index)
            .unwrap();
        chunks[min_index].push((file, size));
        chunk_sizes[min_index] += size;
    }

    debug!("Setting the progress bar");
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) ({bytes_per_sec})",
        )
        .unwrap()
        .progress_chars("#>-"),
    );

    debug!("Sending chunk to parallel processors");
    chunks.into_par_iter().for_each(|chunk| {
        sync_files(
            &chunk,
            &source,
            &destination,
            &Some(pb.clone()),
            args.no_verify,
            args.dry_run,
        );
    });

    pb.finish();
}

#[cfg(test)]
mod tests {

    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_copy_simple_files() {
        let temp_dest = TempDir::new().unwrap();
        let source = Path::new("data/simple_files");
        let destination = temp_dest.path();

        let mut files = get_file_list(&source, None, None);
        let num_threads = std::thread::available_parallelism()
            .unwrap_or_else(|_| NonZeroUsize::new(1).unwrap())
            .get();

        // Sort files by size (largest first) for better distribution
        files.sort_by(|a, b| b.1.cmp(&a.1));

        // Distribute files across threads by balancing total size
        let mut chunks = vec![vec![]; num_threads];
        let mut chunk_sizes = vec![0; num_threads];
        for (file, size) in files {
            let min_index = chunk_sizes
                .iter()
                .enumerate()
                .min_by_key(|&(_, &size)| size)
                .map(|(index, _)| index)
                .unwrap();
            chunks[min_index].push((file, size));
            chunk_sizes[min_index] += size;
        }

        chunks.into_par_iter().for_each(|chunk| {
            sync_files(&chunk, &source, &destination, &None, true, false);
        });

        assert!(compare_dirs(source, destination));
    }

    #[test]
    fn test_copy_symlink() {
        let temp_dest = TempDir::new().unwrap();
        let source = Path::new("data/symlinks");
        let destination = temp_dest.path();

        let mut files = get_file_list(&source, None, None);
        let num_threads = std::thread::available_parallelism()
            .unwrap_or_else(|_| NonZeroUsize::new(1).unwrap())
            .get();

        // Sort files by size (largest first) for better distribution
        files.sort_by(|a, b| b.1.cmp(&a.1));

        // Distribute files across threads by balancing total size
        let mut chunks = vec![vec![]; num_threads];
        let mut chunk_sizes = vec![0; num_threads];
        for (file, size) in files {
            let min_index = chunk_sizes
                .iter()
                .enumerate()
                .min_by_key(|&(_, &size)| size)
                .map(|(index, _)| index)
                .unwrap();
            chunks[min_index].push((file, size));
            chunk_sizes[min_index] += size;
        }

        chunks.into_par_iter().for_each(|chunk| {
            sync_files(&chunk, &source, &destination, &None, true, false);
        });

        assert!(compare_dirs(source, destination));
    }

    #[test]
    fn test_copy_broken_symlink() {
        let temp_dest = TempDir::new().unwrap();
        let source = Path::new("data/broken_symlinks");
        let destination = temp_dest.path();

        let mut files = get_file_list(&source, None, None);
        let num_threads = std::thread::available_parallelism()
            .unwrap_or_else(|_| NonZeroUsize::new(1).unwrap())
            .get();

        // Sort files by size (largest first) for better distribution
        files.sort_by(|a, b| b.1.cmp(&a.1));

        // Distribute files across threads by balancing total size
        let mut chunks = vec![vec![]; num_threads];
        let mut chunk_sizes = vec![0; num_threads];
        for (file, size) in files {
            let min_index = chunk_sizes
                .iter()
                .enumerate()
                .min_by_key(|&(_, &size)| size)
                .map(|(index, _)| index)
                .unwrap();
            chunks[min_index].push((file, size));
            chunk_sizes[min_index] += size;
        }

        chunks.into_par_iter().for_each(|chunk| {
            sync_files(&chunk, &source, &destination, &None, true, false);
        });

        assert!(compare_dirs(source, destination));
    }

    #[test]
    fn test_copy_hidden_files() {
        let temp_dest = TempDir::new().unwrap();
        let source = Path::new("data/hidden_files");
        let destination = temp_dest.path();

        let mut files = get_file_list(&source, None, None);
        let num_threads = std::thread::available_parallelism()
            .unwrap_or_else(|_| NonZeroUsize::new(1).unwrap())
            .get();

        // Sort files by size (largest first) for better distribution
        files.sort_by(|a, b| b.1.cmp(&a.1));

        // Distribute files across threads by balancing total size
        let mut chunks = vec![vec![]; num_threads];
        let mut chunk_sizes = vec![0; num_threads];
        for (file, size) in files {
            let min_index = chunk_sizes
                .iter()
                .enumerate()
                .min_by_key(|&(_, &size)| size)
                .map(|(index, _)| index)
                .unwrap();
            chunks[min_index].push((file, size));
            chunk_sizes[min_index] += size;
        }

        chunks.into_par_iter().for_each(|chunk| {
            sync_files(&chunk, &source, &destination, &None, true, false);
        });

        assert!(compare_dirs(source, destination));
    }

    #[test]
    fn test_copy_deep_directory() {
        let temp_dest = TempDir::new().unwrap();
        let source = Path::new("data/deep_directory");
        let destination = temp_dest.path();

        let mut files = get_file_list(&source, None, None);
        let num_threads = std::thread::available_parallelism()
            .unwrap_or_else(|_| NonZeroUsize::new(1).unwrap())
            .get();

        // Sort files by size (largest first) for better distribution
        files.sort_by(|a, b| b.1.cmp(&a.1));

        // Distribute files across threads by balancing total size
        let mut chunks = vec![vec![]; num_threads];
        let mut chunk_sizes = vec![0; num_threads];
        for (file, size) in files {
            let min_index = chunk_sizes
                .iter()
                .enumerate()
                .min_by_key(|&(_, &size)| size)
                .map(|(index, _)| index)
                .unwrap();
            chunks[min_index].push((file, size));
            chunk_sizes[min_index] += size;
        }

        chunks.into_par_iter().for_each(|chunk| {
            sync_files(&chunk, &source, &destination, &None, true, false);
        });

        assert!(compare_dirs(source, destination));
    }
}
