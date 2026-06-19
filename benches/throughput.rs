use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use parsync::{
    backends::{LocalBackend, SshBackend, StorageBackend},
    CopyOptions,
};
use std::sync::Arc;
use tempfile::TempDir;

const THREADS: usize = 4;
const FILE_COUNT: usize = 100;
const FILE_SIZE: usize = 64 * 1024; // 64 KiB
const TOTAL_BYTES: u64 = (FILE_COUNT * FILE_SIZE) as u64;

fn fill_tree(dir: &std::path::Path, count: usize, file_size: usize) {
    std::fs::create_dir_all(dir).unwrap();
    let data: Vec<u8> = (0..file_size).map(|i| (i & 0xFF) as u8).collect();
    for i in 0..count {
        std::fs::write(dir.join(format!("{i:04}.bin")), &data).unwrap();
    }
}

fn copy_opts() -> CopyOptions<'static> {
    CopyOptions {
        threads: THREADS,
        include: None,
        exclude: None,
        dry_run: false,
        no_progress: true,
        no_preserve_times: true,
    }
}

fn local_backend() -> Arc<dyn StorageBackend + Send + Sync> {
    Arc::new(LocalBackend::new())
}

// ── copy: local → local ──────────────────────────────────────────────────────

fn bench_copy_local(c: &mut Criterion) {
    let src_dir = TempDir::new().unwrap();
    fill_tree(src_dir.path(), FILE_COUNT, FILE_SIZE);
    let src_path = src_dir.path().to_str().unwrap().to_string();
    let opts = copy_opts();

    let mut g = c.benchmark_group("copy");
    g.throughput(Throughput::Bytes(TOTAL_BYTES));
    g.sample_size(20);

    g.bench_function("local", |b| {
        b.iter_batched(
            || TempDir::new().unwrap(),
            |dst| {
                let dst_path = dst.path().join("out");
                parsync::copy(
                    local_backend(),
                    &src_path,
                    local_backend(),
                    dst_path.to_str().unwrap(),
                    &opts,
                )
                .unwrap();
                dst
            },
            BatchSize::SmallInput,
        )
    });

    g.finish();
}

// ── copy: local → ssh://127.0.0.1 ────────────────────────────────────────────

fn bench_copy_ssh(c: &mut Criterion) {
    let user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
    let ssh_b: Arc<dyn StorageBackend + Send + Sync> =
        match SshBackend::connect(&user, "127.0.0.1", 22, THREADS) {
            Ok(b) => Arc::new(b),
            Err(e) => {
                eprintln!("skipping ssh bench ({e:?})");
                return;
            }
        };

    let src_dir = TempDir::new().unwrap();
    fill_tree(src_dir.path(), FILE_COUNT, FILE_SIZE);
    let src_path = src_dir.path().to_str().unwrap().to_string();
    let opts = copy_opts();

    let mut g = c.benchmark_group("copy");
    g.throughput(Throughput::Bytes(TOTAL_BYTES));
    g.sample_size(10);

    g.bench_function("ssh_loopback", |b| {
        b.iter_batched(
            || TempDir::new().unwrap(),
            |dst| {
                let dst_path = dst.path().join("out");
                parsync::copy(
                    local_backend(),
                    &src_path,
                    Arc::clone(&ssh_b),
                    dst_path.to_str().unwrap(),
                    &opts,
                )
                .unwrap();
                dst
            },
            BatchSize::PerIteration,
        )
    });

    g.finish();
}

// ── sync: already up-to-date (skip-only path) ────────────────────────────────

fn bench_sync_noop(c: &mut Criterion) {
    let src_dir = TempDir::new().unwrap();
    fill_tree(src_dir.path(), FILE_COUNT, FILE_SIZE);
    let src_path = src_dir.path().to_str().unwrap().to_string();

    let dst_dir = TempDir::new().unwrap();
    let dst_path = dst_dir.path().join("out").to_str().unwrap().to_string();
    let opts = copy_opts();

    parsync::copy(
        local_backend(),
        &src_path,
        local_backend(),
        &dst_path,
        &opts,
    )
    .unwrap();

    let mut g = c.benchmark_group("sync");
    g.throughput(Throughput::Bytes(TOTAL_BYTES));
    g.sample_size(50);

    g.bench_function("local_noop", |b| {
        b.iter(|| {
            parsync::sync(
                local_backend(),
                &src_path,
                local_backend(),
                &dst_path,
                parsync::sync::DEFAULT_CHUNK_SIZE,
                true,
            )
            .unwrap()
        })
    });

    g.finish();
}

// ── delete ────────────────────────────────────────────────────────────────────

fn bench_delete_local(c: &mut Criterion) {
    let src_dir = TempDir::new().unwrap();
    fill_tree(src_dir.path(), FILE_COUNT, FILE_SIZE);
    let src_path = src_dir.path().to_str().unwrap().to_string();
    let opts = copy_opts();

    let mut g = c.benchmark_group("delete");
    g.throughput(Throughput::Bytes(TOTAL_BYTES));
    g.sample_size(20);

    g.bench_function("local", |b| {
        b.iter_batched(
            || {
                let dst = TempDir::new().unwrap();
                let dst_path = dst.path().join("out").to_str().unwrap().to_string();
                parsync::copy(
                    local_backend(),
                    &src_path,
                    local_backend(),
                    &dst_path,
                    &opts,
                )
                .unwrap();
                (dst, dst_path)
            },
            |(dst, dst_path)| {
                parsync::delete(
                    local_backend(),
                    &[dst_path],
                    THREADS,
                    false,
                    true,
                    None,
                    None,
                )
                .unwrap();
                dst
            },
            BatchSize::PerIteration,
        )
    });

    g.finish();
}

criterion_group!(
    benches,
    bench_copy_local,
    bench_copy_ssh,
    bench_sync_noop,
    bench_delete_local,
);
criterion_main!(benches);
