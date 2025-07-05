# Parsync

## To run in development
```bash
uv run parsync --help

```

## To install
```bash
uv tool install .
```

## Usage
```bash
# Basic usage
smart-rsync /source/ /destination/

# Advanced options
smart-rsync -t 4 --chunk-size 200 --rsync-args "-av --exclude=*.tmp" /src/ remote:/dst/

# Dry run
smart-rsync --dry-run /source/ /destination/
```
