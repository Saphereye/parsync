use ascii_table::{Align, AsciiTable};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, info};
use rayon::prelude::*;
use std::fmt::Display;
use std::num::NonZeroUsize;
use std::path::{PathBuf};

mod protocols;
mod utils;

use crate::protocols::local_protocol::LocalProtocal;
use crate::protocols::protocol::Protocol;

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
    let source = PathBuf::from(args.source);
    debug!("Set source as: {:?}", source);
    let destination = PathBuf::from(args.destination);
    debug!("Set destination as: {:?}", destination);

    if args.diff {
        LocalProtocal::compare_dirs(&source, &destination);
        return;
    }

    let mut files = LocalProtocal::get_file_list(&source, Some(&destination), args.include, args.exclude, args.no_verify);
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
        LocalProtocal::sync_files(
            &chunk,
            &source,
            &destination,
            &Some(pb.clone()),
            args.dry_run,
        );
    });

    pb.finish();
}
