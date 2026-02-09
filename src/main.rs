use clap::{Parser, Subcommand};
use parsync::backends::backend_and_path;
use std::num::NonZeroUsize;

mod backends;
mod sync;
mod utils;

/// Command-line interface for parsync
#[derive(Parser)]
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

    /// Enables dry-run mode
    #[arg(long, global = true)]
    dry_run: bool,

    /// Disables the progress bar
    #[arg(long, global = true)]
    no_progress: bool,

    /// Enables diffing of source and destination directories
    #[arg(long, global = true)]
    diff: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Copy files from source(s) to destination
    Copy {
        /// Source path(s). Globs are expanded for local files. (e.g., src/* or libvb_*)
        sources: Vec<String>,
        /// Destination path (supports local paths and URIs, e.g., file:///path/to/dest)
        destination: String,
    },
    /// Delete files or directories recursively
    Delete {
        /// One or more paths to delete. Globs are expanded for local files.
        paths: Vec<String>,
    },
    /// Sync only those files which differ
    Sync {
        /// Source path(s). Globs are expanded for local files.
        sources: Vec<String>,
        /// Destination path (e.g., file:///path/to/dest)
        destination: String,
    },
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::new().filter("PARSYNC_LOG")).init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Copy {
            sources,
            destination,
        } => {
            use glob::glob;
            use std::collections::BTreeSet;
            use std::fs;

            // Expand all globs and deduplicate
            let mut all_sources = BTreeSet::new();
            let mut backend_opt = None;

            for source in sources {
                let is_glob = source.contains('*') || source.contains('?') || source.contains('[');
                let is_local = !source.contains("://");
                let mut expanded = Vec::new();

                if is_glob && is_local {
                    match glob(&source) {
                        Ok(paths_iter) => {
                            for entry in paths_iter.filter_map(Result::ok) {
                                if let Some(s) = entry.to_str() {
                                    expanded.push(s.to_string());
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Invalid glob pattern '{}': {}", source, e);
                            continue;
                        }
                    }
                } else {
                    expanded.push(source.clone());
                }

                for expanded_path in expanded {
                    match backend_and_path(&expanded_path) {
                        Ok((b, p)) => {
                            if backend_opt.is_none() {
                                backend_opt = Some(b);
                            }
                            all_sources.insert(p.to_string());
                        }
                        Err(e) => {
                            eprintln!("Invalid source '{}': {:?}", expanded_path, e);
                        }
                    }
                }
            }

            if all_sources.is_empty() || backend_opt.is_none() {
                eprintln!("No valid sources to copy.");
                std::process::exit(1);
            }

            let (dst_backend, dst_path) = match backend_and_path(&destination) {
                Ok((b, p)) => (b, p),
                Err(e) => {
                    eprintln!("Invalid destination: {:?}", e);
                    return;
                }
            };

            // If multiple sources, destination must be a directory
            if all_sources.len() > 1 {
                let meta = fs::metadata(dst_path);
                if meta.as_ref().map(|m| !m.is_dir()).unwrap_or(false) {
                    eprintln!("Destination must be a directory when copying multiple sources.");
                    std::process::exit(1);
                }
            }

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

            let src_backend = backend_opt.unwrap();

            // If only one source, use original logic
            if all_sources.len() == 1 {
                let src_path = all_sources.iter().next().unwrap();
                match parsync::copy(
                    src_backend.clone(),
                    src_path,
                    dst_backend,
                    dst_path,
                    &options,
                ) {
                    Ok(_) => println!("Copy completed successfully."),
                    Err(e) => eprintln!("Copy failed: {:?}", e),
                }
            } else {
                // Multiple sources: copy each into destination directory
                let mut any_failed = false;
                for src_path in all_sources {
                    let file_name = std::path::Path::new(&src_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");
                    let mut dst_file_path = std::path::PathBuf::from(dst_path);
                    dst_file_path.push(file_name);
                    match parsync::copy(
                        src_backend.clone(),
                        &src_path,
                        dst_backend.clone(),
                        dst_file_path.to_str().unwrap(),
                        &options,
                    ) {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("Copy failed for '{}': {:?}", src_path, e);
                            any_failed = true;
                        }
                    }
                }
                if any_failed {
                    std::process::exit(1);
                } else {
                    println!("Copy completed successfully.");
                }
            }
        }
        Commands::Sync {
            sources,
            destination,
        } => {
            use glob::glob;
            use std::collections::BTreeSet;
            use std::fs;

            // Expand all globs and deduplicate
            let mut all_sources = BTreeSet::new();
            let mut backend_opt = None;

            for source in sources {
                let is_glob = source.contains('*') || source.contains('?') || source.contains('[');
                let is_local = !source.contains("://");
                let mut expanded = Vec::new();

                if is_glob && is_local {
                    match glob(&source) {
                        Ok(paths_iter) => {
                            for entry in paths_iter.filter_map(Result::ok) {
                                if let Some(s) = entry.to_str() {
                                    expanded.push(s.to_string());
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Invalid glob pattern '{}': {}", source, e);
                            continue;
                        }
                    }
                } else {
                    expanded.push(source.clone());
                }

                for expanded_path in expanded {
                    match backend_and_path(&expanded_path) {
                        Ok((b, p)) => {
                            if backend_opt.is_none() {
                                backend_opt = Some(b);
                            }
                            all_sources.insert(p.to_string());
                        }
                        Err(e) => {
                            eprintln!("Invalid source '{}': {:?}", expanded_path, e);
                        }
                    }
                }
            }

            if all_sources.is_empty() || backend_opt.is_none() {
                eprintln!("No valid sources to sync.");
                std::process::exit(1);
            }

            let (dst_backend, dst_path) = match backend_and_path(&destination) {
                Ok((b, p)) => (b, p),
                Err(e) => {
                    eprintln!("Invalid destination: {:?}", e);
                    std::process::exit(1);
                }
            };

            // If multiple sources, destination must be a directory
            if all_sources.len() > 1 {
                let meta = fs::metadata(dst_path);
                if meta.as_ref().map(|m| !m.is_dir()).unwrap_or(false) {
                    eprintln!("Destination must be a directory when syncing multiple sources.");
                    std::process::exit(1);
                }
            }

            let src_backend = backend_opt.unwrap();

            // If only one source, use original logic
            if all_sources.len() == 1 {
                let src_path = all_sources.iter().next().unwrap();
                let result = parsync::sync(
                    src_backend.clone(),
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
            } else {
                // Multiple sources: sync each into destination directory
                let mut any_failed = false;
                for src_path in all_sources {
                    let file_name = std::path::Path::new(&src_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");
                    let mut dst_file_path = std::path::PathBuf::from(dst_path);
                    dst_file_path.push(file_name);
                    let result = parsync::sync(
                        src_backend.clone(),
                        &src_path,
                        dst_backend.clone(),
                        dst_file_path.to_str().unwrap(),
                        parsync::sync::DEFAULT_CHUNK_SIZE,
                        cli.no_progress,
                    );
                    match result {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("Sync failed for '{}': {:?}", src_path, e);
                            any_failed = true;
                        }
                    }
                }
                if any_failed {
                    std::process::exit(1);
                } else {
                    println!("Sync completed successfully.");
                }
            }
        }
        Commands::Delete { paths } => {
            use glob::glob;

            /* Compile include/exclude regex filters if provided */
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

            let mut all_paths = std::collections::BTreeSet::new();
            let mut backend_opt = None;

            for path in paths {
                let is_glob = path.contains('*') || path.contains('?') || path.contains('[');
                let is_local = !path.contains("://");
                let mut expanded_paths = Vec::new();

                if is_glob && is_local {
                    match glob(&path) {
                        Ok(paths_iter) => {
                            for entry in paths_iter.filter_map(Result::ok) {
                                if let Some(s) = entry.to_str() {
                                    expanded_paths.push(s.to_string());
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Invalid glob pattern '{}': {}", path, e);
                            continue;
                        }
                    }
                } else {
                    expanded_paths.push(path.clone());
                }

                for expanded_path in expanded_paths {
                    match backend_and_path(&expanded_path) {
                        Ok((b, p)) => {
                            if backend_opt.is_none() {
                                backend_opt = Some(b);
                            }
                            // Only allow all paths to use the same backend instance
                            all_paths.insert(p.to_string());
                        }
                        Err(e) => {
                            eprintln!("Invalid path '{}': {:?}", expanded_path, e);
                        }
                    }
                }
            }

            if all_paths.is_empty() || backend_opt.is_none() {
                eprintln!("No valid paths to delete.");
                std::process::exit(1);
            }

            let backend = backend_opt.unwrap();
            let root_vec: Vec<String> = all_paths.into_iter().collect();

            match parsync::delete(
                backend,
                &root_vec,
                threads,
                dry_run,
                no_progress,
                include_re.as_ref(),
                exclude_re.as_ref(),
            ) {
                Ok(_) => println!("Delete completed successfully."),
                Err(e) => {
                    eprintln!("Delete failed: {:?}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
