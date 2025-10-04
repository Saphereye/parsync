use ascii_table::{Align, AsciiTable};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, info, warn};
use rayon::prelude::*;
use std::fmt::Display;
use std::num::NonZeroUsize;
use std::path::PathBuf;

mod protocols;
mod utils;

use crate::protocols::local_sink::LocalSink;
use crate::protocols::local_source::LocalSource;
use crate::protocols::ssh_sink::SSHSink;
use crate::protocols::ssh_source::SSHSource;
use crate::protocols::synchronizer::Synchronizer;

#[derive(Debug, Clone, Copy, PartialEq)]
enum ProtocolType {
    Local,
    SSH,
}

/// Parse path specification to determine protocol type
/// 
/// SSH format: user@host:path
/// Local format: /path/to/dir or ./relative/path
fn parse_path_spec(spec: &str) -> (ProtocolType, String) {
    // Check if it matches SSH format (user@host:path)
    if spec.contains('@') && spec.contains(':') {
        let parts: Vec<&str> = spec.split('@').collect();
        if parts.len() == 2 {
            let host_path: Vec<&str> = parts[1].split(':').collect();
            if host_path.len() == 2 {
                return (ProtocolType::SSH, spec.to_string());
            }
        }
    }
    
    // Otherwise it's local
    (ProtocolType::Local, spec.to_string())
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
    
    // Parse source and destination to determine protocol types
    let (source_proto, source_spec) = parse_path_spec(&args.source);
    let (dest_proto, dest_spec) = parse_path_spec(&args.destination);
    
    debug!("Source: {:?} ({})", source_proto, source_spec);
    debug!("Destination: {:?} ({})", dest_proto, dest_spec);

    // Handle different protocol combinations
    match (source_proto, dest_proto) {
        (ProtocolType::Local, ProtocolType::Local) => {
            sync_local_to_local(
                &source_spec,
                &dest_spec,
                &args,
            );
        }
        (ProtocolType::Local, ProtocolType::SSH) => {
            sync_local_to_ssh(
                &source_spec,
                &dest_spec,
                &args,
            );
        }
        (ProtocolType::SSH, ProtocolType::Local) => {
            sync_ssh_to_local(
                &source_spec,
                &dest_spec,
                &args,
            );
        }
        (ProtocolType::SSH, ProtocolType::SSH) => {
            eprintln!("SSH to SSH synchronization is not yet supported");
            std::process::exit(1);
        }
    }
}

/// Synchronize from local source to local destination
fn sync_local_to_local(source_spec: &str, dest_spec: &str, args: &Args) {
    let source = PathBuf::from(source_spec);
    let destination = PathBuf::from(dest_spec);
    
    debug!("Set source as: {:?}", source);
    debug!("Set destination as: {:?}", destination);

    if args.diff {
        info!("Running diff mode for local-to-local sync");
        Synchronizer::<LocalSource, LocalSink>::compare_dirs_local(&source, &destination);
        return;
    }

    let source_impl = LocalSource::new(source.clone());
    let sink_impl = LocalSink::new(destination.clone());
    let synchronizer = Synchronizer::new(source_impl, sink_impl);

    let files = synchronizer.get_files_to_sync(
        &source,
        &destination,
        args.include.clone(),
        args.exclude.clone(),
        args.no_verify,
    );
    
    sync_files_common(files, &source, &destination, args, synchronizer);
}

/// Synchronize from local source to SSH destination
fn sync_local_to_ssh(source_spec: &str, dest_spec: &str, args: &Args) {
    let source = PathBuf::from(source_spec);
    
    let sink_impl = match SSHSink::new(dest_spec) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to parse SSH destination: {}", e);
            std::process::exit(1);
        }
    };
    
    let destination = sink_impl.root().clone();
    
    debug!("Set source as: {:?}", source);
    debug!("Set destination as: {} (SSH)", dest_spec);

    // Warn about unsupported features for SSH
    if args.diff {
        warn!("Diff mode is not supported for SSH destinations, ignoring --diff flag");
    }

    let source_impl = LocalSource::new(source.clone());
    let synchronizer = Synchronizer::new(source_impl, sink_impl);

    let files = synchronizer.get_files_to_sync(
        &source,
        &destination,
        args.include.clone(),
        args.exclude.clone(),
        args.no_verify,
    );
    
    sync_files_common(files, &source, &destination, args, synchronizer);
}

/// Synchronize from SSH source to local destination
fn sync_ssh_to_local(source_spec: &str, dest_spec: &str, args: &Args) {
    let destination = PathBuf::from(dest_spec);
    
    let source_impl = match SSHSource::new(source_spec) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to parse SSH source: {}", e);
            std::process::exit(1);
        }
    };
    
    let source = source_impl.root().clone();
    
    debug!("Set source as: {} (SSH)", source_spec);
    debug!("Set destination as: {:?}", destination);

    // Warn about unsupported features for SSH
    if args.diff {
        warn!("Diff mode is not supported for SSH sources, ignoring --diff flag");
    }

    let sink_impl = LocalSink::new(destination.clone());
    let synchronizer = Synchronizer::new(source_impl, sink_impl);

    let files = synchronizer.get_files_to_sync(
        &source,
        &destination,
        args.include.clone(),
        args.exclude.clone(),
        args.no_verify,
    );
    
    sync_files_common(files, &source, &destination, args, synchronizer);
}

/// Common sync logic for all protocol combinations
fn sync_files_common<S, D>(
    mut files: Vec<(PathBuf, u64)>,
    source: &PathBuf,
    destination: &PathBuf,
    args: &Args,
    synchronizer: Synchronizer<S, D>,
)
where
    S: crate::protocols::source::Source + Send + Sync,
    D: crate::protocols::sink::Sink + Send + Sync,
{
    let total_size: u64 = files.iter().map(|(_, size)| *size).sum();
    info!(
        "Found {} files, with total size relevant files: {}",
        files.len(),
        utils::size_to_human_readable(total_size as f64)
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
                Box::new(utils::size_to_human_readable(chunk_size as f64)),
                Box::new(utils::size_to_human_readable(avg_size)),
                Box::new(utils::size_to_human_readable(std_dev)),
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
        info!("Total Size: {:>10}", utils::size_to_human_readable(total as f64));
        info!(
            "Min Chunk Size: {:>10}",
            utils::size_to_human_readable(min_size as f64)
        );
        info!(
            "Max Chunk Size: {:>10}",
            utils::size_to_human_readable(max_size as f64)
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
        synchronizer.sync_files(
            &chunk,
            source,
            destination,
            &Some(pb.clone()),
            args.dry_run,
        );
    });

    pb.finish();
}
