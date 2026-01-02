# Parsync
Parsync is file copying tool with speed being the highest priority.

## Usage
You can get the complete list of supported options using the `--help` command.
But essentially, parsync provides the following subcommands:

```
Usage: parsync [OPTIONS] <COMMAND>

Commands:
  copy    Copy files from source to destination (supports local paths and backend URIs)
  delete  Delete files or directories recursively
  sync    Sync a file from source to destination using chunked hashing
  help    Print this message or the help of the given subcommand(s)
...
```

The difference between copying and syncing is that sync will compare the source and th
destination files, and thus should make the copying faster, as we only send the files
which are different.

## Build
The install you can clone the repo and install it using cargo or use the
git subcommand in cargo to install directly.
```bash
cargo install --path . --locked
```

## Benchmarks
These are crude benchmarks using the `time` utility. The source is a folder
of size ~8GiB. These tests were done on an 11th Gen i3.

### Fresh copy of folder
```bash
parsync copy ~/Downloads ~/Downloads_copy  0.41s user 7.82s system 82% cpu 10.001 total
```

### Sync after fresh copy
```bash
parsync sync ~/Downloads ~/Downloads_copy  0.14s user 0.66s system 142% cpu 0.560 total
```

### Delete of copy
```bash
parsync delete ~/Downloads_copy  0.19s user 1.21s system 276% cpu 0.507 total
```
