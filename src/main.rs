use ascii_table::{Align, AsciiTable};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use log::info;
use rayon::prelude::*;
use std::fmt::Display;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

mod protocols;
mod utils;

use crate::protocols::{sync, LocalProtocol, Source};
use crate::utils::size_to_human_readable;

/// Clap args
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short, long)]
    source: String,

    #[arg(short, long)]
    destination: String,

    #[arg(short, long, default_value_t = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1))]
    threads: usize,

    #[arg(long)]
    no_verify: bool,

    #[arg(long)]
    verbose: bool,

    #[arg(short, long)]
    include: Option<String>,

    #[arg(short, long)]
    exclude: Option<String>,

    #[arg(long)]
    dry_run: bool,

    #[arg(long)]
    diagnostics: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let mut builder = env_logger::Builder::new();
    builder.filter_level(log::LevelFilter::Error);
    if args.verbose {
        builder.filter_level(log::LevelFilter::Debug);
    }
    if args.diagnostics {
        builder.filter_level(log::LevelFilter::Info);
    }
    builder.init();

    let source_path = PathBuf::from(args.source);
    let dest_path = PathBuf::from(args.destination);

    let protocol = LocalProtocol;

    let files = protocol.list_files(
        &source_path,
        args.include.as_deref(),
        args.exclude.as_deref(),
    )?;

    let total_size: u64 = files.iter().map(|(entry, _)| entry.size).sum();
    info!(
        "Found {} relevant files, total size: {}",
        files.len(),
        size_to_human_readable(total_size as f64)
    );

    // Sort by size for more balanced threading
    let mut files = files;
    files.sort_by(|a, b| b.0.size.cmp(&a.0.size));

    // Divide files into N threads by size
    let mut chunks = vec![vec![]; args.threads];
    let mut sizes = vec![0u64; args.threads];

    for (entry, _) in files {
        let min_i = sizes
            .iter()
            .enumerate()
            .min_by_key(|(_, &s)| s)
            .map(|(i, _)| i)
            .unwrap();
        sizes[min_i] += entry.size;
        chunks[min_i].push(entry);
    }

    if args.diagnostics {
        let mut table_rows: Vec<Vec<Box<dyn Display>>> = Vec::new();

        for (i, chunk) in chunks.iter().enumerate() {
            let sizes: Vec<u64> = chunk.iter().map(|f| f.size).collect();
            let total = sizes.iter().sum::<u64>();
            let avg = total as f64 / sizes.len().max(1) as f64;
            let std_dev = if sizes.len() > 1 {
                let var = sizes.iter().map(|&s| (s as f64 - avg).powi(2)).sum::<f64>()
                    / (sizes.len() as f64 - 1.0);
                var.sqrt()
            } else {
                0.0
            };

            table_rows.push(vec![
                Box::new(i),
                Box::new(sizes.len()),
                Box::new(size_to_human_readable(total as f64)),
                Box::new(size_to_human_readable(avg)),
                Box::new(size_to_human_readable(std_dev)),
            ]);
        }

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

        table.print(table_rows);
    }

    // Setup progress bar
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) ({bytes_per_sec})",
        )
        .unwrap()
        .progress_chars("##-"),
    );

    chunks.into_par_iter().for_each(|chunk| {
        sync(&protocol, &protocol, chunk, args.dry_run, &Some(pb.clone())).unwrap();
    });

    pb.finish_with_message("Done!");
    Ok(())
}
