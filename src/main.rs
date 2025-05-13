use ascii_table::{Align, AsciiTable};
use blake3::Hasher;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, error, info};
use rayon::prelude::*;
use regex::Regex;
use std::collections::HashSet;
use std::fmt::Display;
use std::fs::{self, File};
use std::io::Read;
use std::num::NonZeroUsize;
use std::ops::Not;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

mod protocols;

/// Get list of all files with their sizes
fn get_file_list(
    source: &Path,
    destination: Option<&Path>,
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
                && path.read_dir().map(|mut i| i.next().is_none()).unwrap_or(false);

            if !(is_file || is_symlink || is_empty_dir) {
                return None;
            }

            if include.as_ref().map(|r| r.is_match(&path_str)).unwrap_or(true)
                && !exclude.as_ref().map(|r| r.is_match(&path_str)).unwrap_or(false)
            {
                if !no_verify && is_file {
                    if let Some(dst_root) = destination {
                        if let Ok(relative) = path.strip_prefix(source) {
                            let dst_path = dst_root.join(relative);
                            if dst_path.exists() {
                                if let (Some(src_hash), Some(dst_hash)) =
                                    (file_checksum(path), file_checksum(&dst_path))
                                {
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

/// Compute Blake3 hash of a file (optional integrity check)
fn file_checksum(path: &Path) -> Option<String> {
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

fn create_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
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

/// Copy files in parallel, considering size-based progress
fn sync_files(
    files: &[(PathBuf, u64)],
    source: &Path,
    destination: &Path,
    pb: &Option<ProgressBar>,
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
                } else if let Err(e) = create_symlink(&target, &dest_file)
 {
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
                } else if let Err(e) = create_symlink(file, &dest_file) {
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

    /// Enables diagnostics for the program
    #[arg(long)]
    diagnostics: bool,
}

fn size_to_human_readable(size: f64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let mut size = size;
    let mut unit = 0;
    while size >= 1024.0 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.2} {}", size, units[unit])
}

#[derive(Debug, PartialEq)]
enum Status {
    Passed,
    Failed,
}

impl Not for Status {
    type Output = Status;

    fn not(self) -> Self::Output {
        match self {
            Status::Passed => Status::Failed,
            Status::Failed => Status::Passed,
        }
    }
}

fn compare_dirs(src: &Path, dest: &Path) -> Status {
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
            let result = compare_file_metadata(src, dest, path);
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

fn compare_file_metadata(src: &Path, dest: &Path, file: &Path) -> Status {
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
            file_checksum(&src_path).ok_or(()),
            file_checksum(&dest_path).ok_or(()),
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

fn main() {
    let args = Args::parse();
    let mut builder = env_logger::Builder::new();
    builder.filter_level(log::LevelFilter::Error); // Show errors by default

    if args.verbose {
        builder.filter_level(log::LevelFilter::Debug);
    }

    if args.diagnostics {
        builder.filter_level(log::LevelFilter::Info);
    }

    builder.init();
    let source = PathBuf::from(args.source);
    debug!("Set source as: {:?}", source);
    let destination = PathBuf::from(args.destination);
    debug!("Set destination as: {:?}", destination);

    if args.diff {
        compare_dirs(&source, &destination);
        return;
    }

    let mut files = get_file_list(&source, Some(&destination), args.include, args.exclude, args.no_verify);
    let total_size: u64 = files.iter().map(|(_, size)| *size).sum();
    info!(
        "Found {} files, with total size relevant files: {}",
        files.len(),
        size_to_human_readable(total_size as f64)
    );
    let num_threads = args.threads;
    info!("Using {} threads for the process", num_threads);

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

    if args.diagnostics {
        let mut max_size = 0;
        let mut min_size = u64::MAX;
        let mut total = 0;
        let mut all_sizes = vec![];
        let mut rows: Vec<Vec<Box<dyn Display>>> = Vec::new();

        for (i, chunk) in chunks.iter().enumerate() {
            let sizes: Vec<u64> = chunk.iter().map(|(_, size)| *size).collect();
            let file_count = sizes.len();
            let chunk_size: u64 = sizes.iter().sum();
            let avg_size = if file_count > 0 {
                chunk_size as f64 / file_count as f64
            } else {
                0.0
            };
            let std_dev = if file_count > 1 {
                let variance = sizes
                    .iter()
                    .map(|s| {
                        let diff = *s as f64 - avg_size;
                        diff * diff
                    })
                    .sum::<f64>()
                    / (file_count as f64 - 1.0);
                variance.sqrt()
            } else {
                0.0
            };

            rows.push(vec![
                Box::new(i),
                Box::new(file_count),
                Box::new(size_to_human_readable(chunk_size as f64)),
                Box::new(size_to_human_readable(avg_size)),
                Box::new(size_to_human_readable(std_dev)),
            ]);

            max_size = max_size.max(chunk_size);
            min_size = min_size.min(chunk_size);
            total += chunk_size;
            all_sizes.push(chunk_size);
        }

        // Print table
        let mut table = AsciiTable::default();
        table.set_max_width(100);
        table.column(0).set_header("Thread").set_align(Align::Right);
        table.column(1).set_header("Files").set_align(Align::Right);
        table
            .column(2)
            .set_header("Total Size")
            .set_align(Align::Right);
        table
            .column(3)
            .set_header("Average")
            .set_align(Align::Right);
        table
            .column(4)
            .set_header("Std Dev")
            .set_align(Align::Right);

        table.print(rows);
        let avg_chunk_size = total as f64 / num_threads as f64;
        info!("Total Size: {:>10}", size_to_human_readable(total as f64));
        info!(
            "Min Chunk Size: {:>10}",
            size_to_human_readable(min_size as f64)
        );
        info!(
            "Max Chunk Size: {:>10}",
            size_to_human_readable(max_size as f64)
        );
        info!(
            "Imbalance Ratio (max/avg): {:.2}",
            max_size as f64 / avg_chunk_size
        );
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
            args.dry_run,
        );
    });

    pb.finish();
}
