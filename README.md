# Parallel file synchronizer
The program aims to improve upon file transfer/copying speeds by leveraging multithreaded options.
Very loose "benchmarking" shows promise, with the following results

| Folder Size | rsync  | parsync | rsync + parallel |
|-------------|--------|---------|------------------|
| 48.54 GiB   | 2m 44s | 47s     | 1m 21s           |

This project doesn't aim to replace rsync, but rather to provide a faster alternative for those who need it (like me).

> **Note:**
> The commands used for the above benchmark were:
> - rsync: `rsync -az --info=progress2 --no-inc-recursive --human-readable --partial /path/to/source /path/to/destination`
> - parsync: `parsync -s /path/to/source --d /path/to/destination --threads 12`
> - parsync + parallel: `cd /path/to/source && \ls -1 | parallel -v -j12 rsync -raz --progress {} /path/to/destination/{}`

## Usage
```bash
parsync [OPTIONS] --source <SOURCE> --destination <DESTINATION>

Options:
  -s, --source <SOURCE>            Source directory
  -d, --destination <DESTINATION>  Destination directory
  -t, --threads <THREADS>          Number of threads to use [default: 12]
      --no-verify                  Disables checksum verification
      --verbose                    Enables verbose output
  -i, --include <INCLUDE>          Regex for files/folders to include
  -e, --exclude <EXCLUDE>          Regex for files/folders to exclude
      --dry-run                    Enables dry-run mode
      --diff                       Enables diffing of source and destination directories
  -h, --help                       Print help
  -V, --version                    Print version
```
