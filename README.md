# parsync

A parallel file synchronizer built for speed. Uses OS-assisted copy fast paths
(`FICLONE`, `copy_file_range`, `sendfile`) on Linux, a producer-consumer thread
pool for copy/delete, and lock-free atomic work-sharing for sync.

## Install

```bash
cargo install --path . --locked
```

## Usage

```
parsync [OPTIONS] <COMMAND>

Commands:
  copy    Copy files from source(s) to destination
  sync    Sync only those files which differ (size + mtime)
  delete  Delete files or directories recursively

Options:
  -t, --threads <N>   Worker threads (default: available CPUs)
  -i, --include <RE>  Only include paths matching regex
  -e, --exclude <RE>  Exclude paths matching regex
      --dry-run       Print what would be done, without doing it
      --no-progress   Suppress progress bar
      --diff          Show diff of source vs destination
```

### Examples

```bash
# Copy a directory
parsync copy ~/src ~/dst

# Sync (skips unchanged files)
parsync sync ~/src ~/dst

# Copy to a remote host over SSH
parsync copy ~/src ssh://user@host/remote/path
parsync copy ~/src ssh://user@host:2222/remote/path

# Delete with glob
parsync delete ~/dst/lib*

# Dry-run with 8 threads
parsync copy --dry-run -t 8 ~/src ~/dst
```

SSH authentication uses the agent, then `~/.ssh/id_ed25519`, `id_ecdsa`, `id_rsa`
in order. The `--threads` flag also sets the SSH connection pool size: each
worker thread gets a dedicated persistent SFTP session, eliminating per-file
subsystem setup round-trips.

## Benchmarks

**Machine:** 11th Gen Intel i3-1115G4 @ 3.00 GHz, 7.4 GiB RAM, Linux 7.0.11  
**Dataset:** 500 files × 256 KiB ≈ 128 MiB, all on **tmpfs (RAM), warm page cache**  
**Tool:** [hyperfine](https://github.com/sharkdp/hyperfine), 7 runs each, 2 warmup runs

> Numbers measure **protocol and threading overhead**, not disk I/O — tmpfs
> eliminates storage latency so kernel scheduling, syscall cost, and wire
> protocol efficiency dominate.

### Local copy (fresh destination)

| Tool | Mean | Relative |
|---|---|---|
| `parsync copy -t 4` | **14.0 ms** | — |
| `cp -r` | 26.0 ms | 1.9× slower |
| `rsync -a --no-compress` | 106.6 ms | 7.6× slower |
| `rclone copy --transfers 4` | 152.6 ms | 10.9× slower |
| `parallel -j4 cp` (GNU parallel) | 1034 ms | 73.8× slower |

`parsync` parallelises `copy_file_range` across N worker threads.  
`cp -r` is single-threaded.  
`rsync` builds a complete file manifest before transferring, adding round-trip overhead.  
`rclone` has a higher per-file goroutine/channel cost than the rsync protocol.  
GNU `parallel cp` pays 500 process-fork costs — not a fair comparison to
in-process parallelism, included to demonstrate why naive parallelism doesn't help.

### Sync (destination already up to date, no-op)

| Tool | Mean | Relative |
|---|---|---|
| `parsync sync -t 4` | **1.6 ms** | — |
| `rsync -a --no-compress` | 46.6 ms | 28× slower |
| `rclone sync --transfers 4` | 48.4 ms | 30× slower |

parsync's skip path: parallel size+mtime comparison via lock-free atomic index.  
rsync and rclone both walk the full tree serially before skipping.

### Delete

| Tool | Mean |
|---|---|
| `rm -rf` | 3.7 ms |
| `parsync delete -t 4` | 5.4 ms |

Within measurement noise on tmpfs. parsync's two-phase pipeline (parallel file
unlink → sequential deepest-first rmdir) avoids the race where `remove_dir_all`
on a parent could pre-empt in-flight file deletions by other workers.

### SSH copy (loopback, 4 parallel SFTP streams, 500 × 256 KiB)

| Tool | Mean | Note |
|---|---|---|
| `rsync -a --no-compress -e ssh` | **470.6 ms** | single stream, custom wire protocol |
| `parsync copy -t 4 ssh://127.0.0.1` | 748.8 ms | 4 streams, SFTP |

On loopback rsync is faster because its custom protocol pipelines file data
without waiting for per-packet acknowledgements. SFTP (used by parsync) requires
a round-trip ack for every 32 KB fragment — on zero-latency loopback this
dominates. On a real network (>10 ms RTT) parsync's multiple parallel streams
saturate available bandwidth while rsync is limited to one stream; the crossover
point where parsync wins is roughly at the latency where per-stream throughput
drops below total_bandwidth / N_streams.

Each connection pre-initialises one persistent SFTP subsystem (cached per pool
slot), so per-file cost is 3 round-trips (open + write + close) rather than
5+ (init + open + write + close + teardown) as in a naive implementation.

## Architecture

```
copy     producer (WalkDir) ──[channel]──► N workers (copy_file_range / SFTP put_stream)
sync     WalkDir scan ──► atomic index ──► N workers (mtime skip or fast copy)
delete   WalkDir scan ──► phase 1: N workers (parallel unlink)
                      ──► phase 2: dirs deepest-first (sequential rmdir)
SSH      Pool: N pre-authenticated sessions, each with one persistent SFTP handle
         per-connection mkdir cache avoids redundant SFTP_MKDIR round-trips
         streaming 1 MiB chunks; no full-file buffering for local→remote copies
```

## Running the benchmarks

```bash
cargo bench --bench throughput
```

SSH benchmarks require passwordless SSH to `127.0.0.1`:

```bash
cat ~/.ssh/id_ed25519.pub >> ~/.ssh/authorized_keys
chmod 600 ~/.ssh/authorized_keys
```
