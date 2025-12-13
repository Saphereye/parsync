use clap::{Parser, Subcommand};
use std::num::NonZeroUsize;

mod backends;

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
    }
}
