use clap::{Parser, Subcommand};
use std::num::NonZeroUsize;

mod backends;
mod sync;
mod utils;

/// Command-line interface for parsync
#[derive(Parser, Debug)]
#[command(name = "parsync", version, about = "A parallel file synchronizer")]
struct Cli {
    /// Number of worker threads to use across operations
    #[arg(short, long, value_name = "THREADS", global = true, default_value_t = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or_else(|_| NonZeroUsize::new(1).unwrap().get()))]
    threads: usize,

    /// Regex pattern to include matching files and directories
    #[arg(short, long, value_name = "INCLUDE", global = true)]
    include: Option<String>,

    /// Do not preserve source file modification times on destination copies
    #[arg(long, global = true)]
    no_preserve_times: bool,

    /// Regex pattern to exclude matching files and directories
    #[arg(short, long, value_name = "EXCLUDE", global = true)]
    exclude: Option<String>,

    /// Enables dry-run mode (global)
    #[arg(long, global = true)]
    dry_run: bool,

    /// Disables the progress bar (global)
    #[arg(long, global = true)]
    no_progress: bool,

    /// Enables diffing of source and destination directories (global)
    #[arg(long, global = true)]
    diff: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Copy files from source to destination (supports local paths and backend URIs)
    Copy {
        /// Source path (supports local paths and URIs, e.g., file:///path/to/source)
        source: String,
        /// Destination path (supports local paths and URIs, e.g., file:///path/to/dest)
        destination: String,
    },
    /// Delete files or directories recursively
    Delete {
        /// Path to delete (supports local paths and URIs, e.g., file:///path/to/delete)
        path: String,
    },
    /// Sync a file from source to destination using chunked hashing
    Sync {
        /// Source path (e.g., file:///path/to/source)
        source: String,
        /// Destination path (e.g., file:///path/to/dest)
        destination: String,
    },
}

/// Parse a protocol-prefixed path and return (protocol, path)
use parsync::backends::backend_and_path;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::new().filter("PARSYNC_LOG")).init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Copy {
            source,
            destination,
        } => {
            let (src_backend, src_path) = match backend_and_path(&source) {
                Ok((b, p)) => (b, p),
                Err(e) => {
                    eprintln!("Invalid source: {:?}", e);
                    return;
                }
            };
            let (dst_backend, dst_path) = match backend_and_path(&destination) {
                Ok((b, p)) => (b, p),
                Err(e) => {
                    eprintln!("Invalid destination: {:?}", e);
                    return;
                }
            };

            // Prepare regex filters
            let include_re = match &cli.include {
                Some(pattern) => match regex::Regex::new(pattern) {
                    Ok(re) => Some(re),
                    Err(e) => {
                        eprintln!("Invalid include regex: {}", e);
                        return;
                    }
                },
                None => None,
            };
            let exclude_re = match &cli.exclude {
                Some(pattern) => match regex::Regex::new(pattern) {
                    Ok(re) => Some(re),
                    Err(e) => {
                        eprintln!("Invalid exclude regex: {}", e);
                        return;
                    }
                },
                None => None,
            };

            let options = parsync::CopyOptions {
                threads: cli.threads,
                include: include_re.as_ref(),
                exclude: exclude_re.as_ref(),
                dry_run: cli.dry_run,
                no_progress: cli.no_progress,
                no_preserve_times: cli.no_preserve_times,
            };

            match parsync::copy(src_backend, src_path, dst_backend, dst_path, &options) {
                Ok(_) => println!("Copy completed successfully."),
                Err(e) => eprintln!("Copy failed: {:?}", e),
            }
        }
        Commands::Sync {
            source,
            destination,
        } => {
            use std::fs;
            let (src_backend, src_path) = match backend_and_path(&source) {
                Ok((b, p)) => (b, p),
                Err(e) => {
                    eprintln!("Invalid source: {:?}", e);
                    return;
                }
            };
            let (dst_backend, dst_path) = match backend_and_path(&destination) {
                Ok((b, p)) => (b, p),
                Err(e) => {
                    eprintln!("Invalid destination: {:?}", e);
                    return;
                }
            };

            // TODO
            let _src_meta = match fs::metadata(src_path) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Failed to stat source: {e}");
                    return;
                }
            };

            let result = parsync::sync(
                src_backend,
                src_path,
                dst_backend,
                dst_path,
                parsync::sync::DEFAULT_CHUNK_SIZE,
                cli.no_progress,
            );

            match result {
                Ok(_) => println!("Sync completed successfully."),
                Err(e) => eprintln!("Sync failed: {:?}", e),
            }
        }
        Commands::Delete { path } => {
            let (backend, real_path) = match backend_and_path(&path) {
                Ok((b, p)) => (b, p),
                Err(e) => {
                    eprintln!("Invalid path: {:?}", e);
                    return;
                }
            };

            // Prepare regex filters
            let include_re = match &cli.include {
                Some(pattern) => match regex::Regex::new(pattern) {
                    Ok(re) => Some(re),
                    Err(e) => {
                        eprintln!("Invalid include regex: {}", e);
                        return;
                    }
                },
                None => None,
            };
            let exclude_re = match &cli.exclude {
                Some(pattern) => match regex::Regex::new(pattern) {
                    Ok(re) => Some(re),
                    Err(e) => {
                        eprintln!("Invalid exclude regex: {}", e);
                        return;
                    }
                },
                None => None,
            };

            let threads = cli.threads;
            let dry_run = cli.dry_run;
            let no_progress = cli.no_progress;

            match parsync::delete(
                backend,
                real_path,
                threads,
                dry_run,
                no_progress,
                include_re.as_ref(),
                exclude_re.as_ref(),
            ) {
                Ok(_) => println!("Delete completed successfully."),
                Err(e) => eprintln!("Delete failed: {:?}", e),
            }
        }
    }
}
