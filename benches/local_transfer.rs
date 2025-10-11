use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkGroup, measurement::WallTime};
use std::fs;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

fn create_test_files(dir: &TempDir, count: usize, size_mb: usize) {
    for i in 0..count {
        let content = vec![b'x'; size_mb * 1024 * 1024];
        fs::write(dir.path().join(format!("file_{}.bin", i)), content).unwrap();
    }
}

fn configure_group(group: &mut BenchmarkGroup<WallTime>) {
    group
        .sample_size(10)
        .measurement_time(Duration::from_secs(60))
        .warm_up_time(Duration::from_secs(5));
}

fn benchmark_1gb_single_file(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_files");
    configure_group(&mut group);
    
    group.bench_function("1gb_single_file", |b| {
        b.iter(|| {
            let source_dir = TempDir::new().unwrap();
            let dest_dir = TempDir::new().unwrap();
            
            create_test_files(&source_dir, 1, 1024);
            
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
    
    group.finish();
}

fn benchmark_2gb_multiple_files(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_files");
    configure_group(&mut group);
    
    group.bench_function("2gb_multiple_files", |b| {
        b.iter(|| {
            let source_dir = TempDir::new().unwrap();
            let dest_dir = TempDir::new().unwrap();
            
            create_test_files(&source_dir, 20, 100);
            
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
    
    group.finish();
}

fn benchmark_5gb_mixed_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_files");
    configure_group(&mut group);
    
    group.bench_function("5gb_mixed_sizes", |b| {
        b.iter(|| {
            let source_dir = TempDir::new().unwrap();
            let dest_dir = TempDir::new().unwrap();
            
            create_test_files(&source_dir, 5, 500);
            create_test_files(&source_dir, 25, 100);
            
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
    
    group.finish();
}

fn benchmark_thread_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("thread_scaling");
    configure_group(&mut group);
    
    for threads in [2, 4, 8] {
        group.bench_function(format!("{}_threads", threads), |b| {
            b.iter(|| {
                let source_dir = TempDir::new().unwrap();
                let dest_dir = TempDir::new().unwrap();
                
                create_test_files(&source_dir, 20, 100);
                
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
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    benchmark_1gb_single_file,
    benchmark_2gb_multiple_files,
    benchmark_5gb_mixed_sizes,
    benchmark_thread_scaling
);
criterion_main!(benches);
