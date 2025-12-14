use clap::{Parser, Subcommand};
use std::num::NonZeroUsize;

mod backends;
mod sync;
mod utils;

/// Command-line interface for parsync
#[derive(Parser, Debug)]
#[command(name = "parsync", version, about = "A parallel file synchronizer")]
struct Cli {
    /// Number of threads to use (global)
    #[arg(short, long, value_name = "THREADS", global = true, default_value_t = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or_else(|_| NonZeroUsize::new(1).unwrap().get()))]
    threads: usize,

    /// Regex for files/folders to include (global)
    #[arg(short, long, value_name = "INCLUDE", global = true)]
    include: Option<String>,

    /// Regex for files/folders to exclude (global)
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
    /// Copy from source to destination
    Copy {
        /// Source path (e.g., file:///path/to/source)
        source: String,
        /// Destination path (e.g., file:///path/to/dest)
        destination: String,
    },
    /// Delete a file or directory
    Delete {
        /// Path to delete (e.g., file:///path/to/delete)
        path: String,
    },
    /// Sync a file from source to destination using chunked hashing
    #[clap(hide = true)]
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
    // Initialize logging using env_logger and PARSYNC_LOG
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

            let result = parsync::sync_dir_chunked(
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
