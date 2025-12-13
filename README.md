# Parallel file synchronizer
This program aims to improve file transfer and copying speeds by leveraging multithreading.

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

## Usage
```bash
parsync [OPTIONS] --source <SOURCE> --destination <DESTINATION>

Options:
  -s, --source <SOURCE>            Source directory
  -d, --destination <DESTINATION>  Destination directory
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
