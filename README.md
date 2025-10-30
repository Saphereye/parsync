# Parallel file synchronizer
This program aims to improve file transfer and copying speeds by leveraging multithreading.

## Usage

### Basic Usage

```bash
parsync [OPTIONS] --source <SOURCE> --destination <DESTINATION>

Options:
  -s, --source <SOURCE>            Source directory (local path or user@host:path for SSH)
  -d, --destination <DESTINATION>  Destination directory (local path or user@host:path for SSH)
  -t, --threads <THREADS>          Number of threads to use
      --no-verify                  Disables checksum verification
      --verbose                    Enables verbose output
  -i, --include <INCLUDE>          Regex for files/folders to include
  -e, --exclude <EXCLUDE>          Regex for files/folders to exclude
      --dry-run                    Enables dry-run mode
      --diff                       Enables diffing of source and destination directories
  -h, --help                       Print help
  -V, --version                    Print version
```

### Examples

#### Local to Local Sync
Synchronize files from one local directory to another:
```bash
parsync -s /path/to/source -d /path/to/destination
```

#### Local to Remote SSH Sync
Push files to a remote server via SSH:
```bash
parsync -s /path/to/local/source -d user@remote.host:/path/to/destination
```

#### Remote SSH to Local Sync
Pull files from a remote server via SSH:
```bash
parsync -s user@remote.host:/path/to/source -d /path/to/local/destination
```

#### With Filtering
Sync only specific files using regex:
```bash
# Include only .txt files
parsync -s /source -d /dest -i "\.txt$"

# Exclude .log files
parsync -s /source -d /dest -e "\.log$"

# Combine include and exclude
parsync -s /source -d /dest -i "\.txt$" -e "temp"
```

#### Dry Run Mode
Preview what would be synchronized without actually copying:
```bash
parsync -s /source -d /dest --dry-run --verbose
```

#### Using Multiple Threads
Specify the number of parallel threads (default: number of CPU cores):
```bash
parsync -s /source -d /dest -t 8
```

### Hash-Based Synchronization

By default, parsync uses Blake3 hashes to determine which files need to be copied:

1. Files at the destination are hashed first
2. Hashes are sent to the source for comparison
3. Only files with different or missing hashes are transferred

This approach:
- Minimizes unnecessary data transfer
- Works efficiently for both local and remote destinations
- Can be disabled with `--no-verify` for faster initial copies

### SSH Configuration

For SSH synchronization:
- Ensure you have SSH access configured (e.g., via SSH keys or SSH agent)
- The format is `user@host:path` (colon-separated)
- Both source and destination can be remote, but at least one should be local
- Uses the ssh2 library for SSH connections and SFTP for file transfers (no external SSH/SCP commands required)

## Benchmarking
This test was run on the following system specs:
| Component | Details                        |
|-----------|--------------------------------|
| OS        | Ubuntu 22.04.5 LTS x86_64      |
| CPU       | Intel(R) Core(TM) i7-10750H (12) @ 5.00 GHz |
| RAM       | 32 GiB            |
| Swap       | 2 GiB            |
| Filesystem       | ext4            |

And, the source folder had the following stats:
| Metric       | Value         |
|--------------|---------------|
| File Count   | 119,847       |
| Min Size     | 0 bytes       |
| Max Size     | 1.06 GB       |
| Mean Size    | 129.94 KB     |
| Total Size   | 15.48 GiB     |

### Initial Copy to a New Location

| Command | Mean [s] | Min [s] | Max [s] | Relative |
|:---|---:|---:|---:|---:|
| `parsync` | 15.426 ± 1.771 | 13.339 | 18.663 | 1.00 |
| `cp` | 35.888 ± 1.109 | 33.319 | 37.489 | 2.33 ± 0.28 |
| `rsync + parallel` | 98.031 ± 8.694 | 86.814 | 112.189 | 6.36 ± 0.73 |
| `rsync` | 121.976 ± 2.400 | 116.618 | 124.165 | 7.91 ± 0.92 |

### Copying to a Location with Pre-existing Files
This test highlights the advantage of tools that use checksums or metadata to skip already copied files, reducing unnecessary data transfer.
| Command | Mean [s] | Min [s] | Max [s] | Relative |
|:---|---:|---:|---:|---:|
| `rsync` | 1.011 ± 0.062 | 0.911 | 1.099 | 1.00 |
| `rsync + parallel` | 1.456 ± 0.327 | 0.729 | 2.056 | 1.44 ± 0.34 |
| `parsync` | 8.534 ± 0.103 | 8.441 | 8.767 | 8.44 ± 0.53 |
| `cp` | 28.182 ± 3.025 | 20.980 | 31.180 | 27.88 ± 3.45 |

### Note
The commands used for the above benchmark were:
- rsync: `rsync -az --info=progress2 --no-inc-recursive --human-readable --partial /path/to/source /path/to/destination`
- cp: `cp /path/to/source /path/to/destination`
- parsync: `parsync -s /path/to/source -d /path/to/destination --threads 12`
- rsync + parallel: `cd /path/to/source && \ls -1 | parallel -v -j12 "mkdir -p /path/to/destination/{} && rsync -raz {} /path/to/destination/{}"`
