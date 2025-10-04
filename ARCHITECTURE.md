# Parsync Modular Architecture

## Overview

Parsync has been refactored to support a modular source/sink architecture, enabling flexible file synchronization between different storage types (local filesystem, SSH, and extensible to future protocols).

## Core Abstractions

### Source Trait

The `Source` trait defines operations for reading files and metadata from a source location:

```rust
pub trait Source: Send + Sync {
    fn get_file_hash(&self, path: &PathBuf) -> Option<String>;
    fn read_file(&self, path: &PathBuf) -> std::io::Result<Vec<u8>>;
    fn is_symlink(&self, path: &PathBuf) -> bool;
    fn read_link(&self, path: &PathBuf) -> std::io::Result<PathBuf>;
    // ... other methods
}
```

### Sink Trait

The `Sink` trait defines operations for writing files to a destination location:

```rust
pub trait Sink: Send + Sync {
    fn file_exists(&self, path: &PathBuf) -> bool;
    fn get_file_hash(&self, path: &PathBuf) -> Option<String>;
    fn create_dir(&self, path: &PathBuf) -> std::io::Result<()>;
    fn create_symlink(&self, target: &PathBuf, link: &PathBuf) -> std::io::Result<()>;
    fn copy_file(&self, source_path: &PathBuf, dest_path: &PathBuf) -> std::io::Result<()>;
    // ... other methods
}
```

## Implementations

### Local Filesystem

- **LocalSource**: Implements `Source` for local filesystem reads
- **LocalSink**: Implements `Sink` for local filesystem writes

### SSH Remote

- **SSHSource**: Implements `Source` for remote SSH reads
  - Uses `ssh` commands to execute remote operations
  - Falls back to local hash computation if `b3sum` is not available remotely
- **SSHSink**: Implements `Sink` for remote SSH writes
  - Uses `scp` for file transfers
  - Uses `ssh` for directory creation and symlink operations

## Synchronizer

The `Synchronizer` struct orchestrates the sync process:

```rust
pub struct Synchronizer<S: Source, D: Sink> {
    source: S,
    sink: D,
}
```

### Hash Comparison Strategy

The synchronizer implements an optimized approach:

1. **List files** at the source (with filtering)
2. **Check existence** at the destination for each file
3. **Fetch hash** from destination if file exists
4. **Compare hash** locally at source
5. **Transfer** only files that are missing or have different hashes

This minimizes bandwidth by:
- Fetching only hashes (not file contents) from destination
- Comparing hashes locally at source
- Transferring only necessary files

## Protocol Detection

The main function automatically detects the protocol based on path format:

- **Local**: `/path/to/dir` or `./relative/path`
- **SSH**: `user@host:/path/to/dir`

## Usage

### Local to Local
```bash
parsync -s /source -d /dest
```

### Local to SSH
```bash
parsync -s /local/source -d user@host:/remote/dest
```

### SSH to Local
```bash
parsync -s user@host:/remote/source -d /local/dest
```

## Extension Points

To add a new protocol (e.g., S3, FTP):

1. Create a struct that implements `Source` trait
2. Create a struct that implements `Sink` trait
3. Add protocol detection logic in `parse_path_spec()`
4. Add a sync function similar to `sync_local_to_ssh()`

## Thread Safety

All implementations are `Send + Sync` to support parallel operations with Rayon.

## Testing

Run the demo:
```bash
cargo build --release
./target/release/parsync -s /tmp/source -d /tmp/dest
```

## Legacy Code

The old `Protocol` trait and implementations are marked as deprecated but kept for backward compatibility. New code should use the `Source` and `Sink` abstractions.
