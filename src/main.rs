use clap::{Arg, Command};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use sha2::{Digest, Sha256};

/// Get list of all files with their sizes
fn get_file_list(source: &Path) -> Vec<(PathBuf, u64)> {
    WalkDir::new(source)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            (e.path().to_path_buf(), size)
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
fn sync_files(files: &[(PathBuf, u64)], source: &Path, destination: &Path, pb: &ProgressBar) {
    let src_arc = source.to_path_buf();
    let dest_arc = destination.to_path_buf();

    files.par_iter().for_each(|(file, size)| {
        let rel_path = file.strip_prefix(&src_arc).unwrap();
        let dest_file = dest_arc.join(rel_path);

        // Ensure destination directory exists
        if let Some(parent) = dest_file.parent() {
            fs::create_dir_all(parent).unwrap();
        }

        // Check if file needs copying
        if dest_file.exists() {
            let src_hash = file_checksum(file);
            let dest_hash = file_checksum(&dest_file);
            if src_hash == dest_hash {
                pb.inc(*size);
                return; // Skip unchanged files
            }
        }

        // Copy file
        if let Err(e) = fs::copy(file, &dest_file) {
            eprintln!("Failed to copy {:?}: {}", file, e);
        }

        pb.inc(*size);
    });
}

fn main() {
    let matches = Command::new("parsync")
        .version("0.1")
        .author("Saphereye")
        .about("Parallel file synchronizer")
        .arg(Arg::new("source")
            .short('s')
            .long("src")
            .value_name("SOURCE")
            .required(true)
            .num_args(1))
        .arg(Arg::new("destination")
            .short('d')
            .long("dest")
            .value_name("DESTINATION")
            .required(true)
            .num_args(1))
        .get_matches();

    let source = PathBuf::from(matches.get_one::<String>("source").unwrap());
    let destination = PathBuf::from(matches.get_one::<String>("destination").unwrap());

    let mut files = get_file_list(&source);
    let total_size: u64 = files.iter().map(|(_, size)| *size).sum();
    let num_threads = std::thread::available_parallelism().unwrap().get();
    
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

    let pb = ProgressBar::new(total_size);
    pb.set_style(ProgressStyle::with_template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        .unwrap()
        .progress_chars("#>-"));

    chunks.into_par_iter().for_each(|chunk| {
        sync_files(&chunk, &source, &destination, &pb);
    });

    pb.finish_with_message("✅ Sync complete");
}

