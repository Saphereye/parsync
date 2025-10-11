use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn create_test_files(dir: &TempDir, count: usize, size: usize) {
    for i in 0..count {
        let content = vec![b'x'; size];
        fs::write(dir.path().join(format!("file_{}.bin", i)), content).unwrap();
    }
}

fn benchmark_small_files(c: &mut Criterion) {
    let mut group = c.benchmark_group("small_files");
    
    for file_count in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(file_count),
            file_count,
            |b, &count| {
                b.iter(|| {
                    let source_dir = TempDir::new().unwrap();
                    let dest_dir = TempDir::new().unwrap();
                    
                    // Create files (1KB each)
                    create_test_files(&source_dir, count, 1024);
                    
                    let output = Command::new(env!("CARGO_BIN_EXE_parsync"))
                        .arg("-s")
                        .arg(source_dir.path())
                        .arg("-d")
                        .arg(dest_dir.path())
                        .output()
                        .expect("Failed to execute parsync");
                    
                    black_box(output.status.success());
                });
            },
        );
    }
    group.finish();
}

fn benchmark_medium_files(c: &mut Criterion) {
    let mut group = c.benchmark_group("medium_files");
    
    for file_count in [10, 25, 50].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(file_count),
            file_count,
            |b, &count| {
                b.iter(|| {
                    let source_dir = TempDir::new().unwrap();
                    let dest_dir = TempDir::new().unwrap();
                    
                    // Create files (100KB each)
                    create_test_files(&source_dir, count, 100 * 1024);
                    
                    let output = Command::new(env!("CARGO_BIN_EXE_parsync"))
                        .arg("-s")
                        .arg(source_dir.path())
                        .arg("-d")
                        .arg(dest_dir.path())
                        .output()
                        .expect("Failed to execute parsync");
                    
                    black_box(output.status.success());
                });
            },
        );
    }
    group.finish();
}

fn benchmark_thread_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("thread_scaling");
    
    for thread_count in [2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(thread_count),
            thread_count,
            |b, &threads| {
                b.iter(|| {
                    let source_dir = TempDir::new().unwrap();
                    let dest_dir = TempDir::new().unwrap();
                    
                    // Create 50 files (10KB each)
                    create_test_files(&source_dir, 50, 10 * 1024);
                    
                    let output = Command::new(env!("CARGO_BIN_EXE_parsync"))
                        .arg("-s")
                        .arg(source_dir.path())
                        .arg("-d")
                        .arg(dest_dir.path())
                        .arg("-t")
                        .arg(threads.to_string())
                        .output()
                        .expect("Failed to execute parsync");
                    
                    black_box(output.status.success());
                });
            },
        );
    }
    group.finish();
}

fn benchmark_nested_directories(c: &mut Criterion) {
    c.bench_function("nested_directories", |b| {
        b.iter(|| {
            let source_dir = TempDir::new().unwrap();
            let dest_dir = TempDir::new().unwrap();
            
            // Create nested directory structure with files
            for i in 0..10 {
                let dir_path = source_dir.path().join(format!("dir_{}", i));
                fs::create_dir(&dir_path).unwrap();
                for j in 0..10 {
                    let content = vec![b'x'; 1024];
                    fs::write(dir_path.join(format!("file_{}.bin", j)), content).unwrap();
                }
            }
            
            let output = Command::new(env!("CARGO_BIN_EXE_parsync"))
                .arg("-s")
                .arg(source_dir.path())
                .arg("-d")
                .arg(dest_dir.path())
                .output()
                .expect("Failed to execute parsync");
            
            black_box(output.status.success());
        });
    });
}

fn benchmark_incremental_sync(c: &mut Criterion) {
    c.bench_function("incremental_sync_no_changes", |b| {
        let source_dir = TempDir::new().unwrap();
        let dest_dir = TempDir::new().unwrap();
        
        // Create files and do initial sync
        create_test_files(&source_dir, 50, 10 * 1024);
        Command::new(env!("CARGO_BIN_EXE_parsync"))
            .arg("-s")
            .arg(source_dir.path())
            .arg("-d")
            .arg(dest_dir.path())
            .output()
            .expect("Failed to execute parsync");
        
        b.iter(|| {
            let output = Command::new(env!("CARGO_BIN_EXE_parsync"))
                .arg("-s")
                .arg(source_dir.path())
                .arg("-d")
                .arg(dest_dir.path())
                .output()
                .expect("Failed to execute parsync");
            
            black_box(output.status.success());
        });
    });
}

criterion_group!(
    benches,
    benchmark_small_files,
    benchmark_medium_files,
    benchmark_thread_count,
    benchmark_nested_directories,
    benchmark_incremental_sync
);
criterion_main!(benches);
