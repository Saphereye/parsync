use std::fs;
use std::time::Instant;
use tempfile::TempDir;

/// Benchmark for local to local file synchronization
/// 
/// These benchmarks measure the performance of parsync for various scenarios.
/// Run with: cargo test --test benchmarks -- --nocapture

#[test]
fn benchmark_sync_small_files() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create 100 small files (1KB each)
    for i in 0..100 {
        let content = "x".repeat(1024);
        fs::write(source_dir.path().join(format!("file_{}.txt", i)), content).unwrap();
    }

    let start = Instant::now();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .output()
        .expect("Failed to execute parsync");
    let duration = start.elapsed();

    assert!(output.status.success());
    println!("Benchmark: 100 small files (1KB each) - Time: {:?}", duration);
}

#[test]
fn benchmark_sync_medium_files() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create 50 medium files (100KB each)
    for i in 0..50 {
        let content = vec![b'x'; 100 * 1024];
        fs::write(source_dir.path().join(format!("file_{}.bin", i)), content).unwrap();
    }

    let start = Instant::now();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .output()
        .expect("Failed to execute parsync");
    let duration = start.elapsed();

    assert!(output.status.success());
    println!("Benchmark: 50 medium files (100KB each) - Time: {:?}", duration);
}

#[test]
fn benchmark_sync_large_files() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create 10 large files (10MB each)
    for i in 0..10 {
        let content = vec![b'x'; 10 * 1024 * 1024];
        fs::write(source_dir.path().join(format!("file_{}.bin", i)), content).unwrap();
    }

    let start = Instant::now();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .output()
        .expect("Failed to execute parsync");
    let duration = start.elapsed();

    assert!(output.status.success());
    println!("Benchmark: 10 large files (10MB each) - Time: {:?}", duration);
}

#[test]
fn benchmark_sync_nested_directories() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create nested directory structure with files
    for i in 0..10 {
        let dir_path = source_dir.path().join(format!("dir_{}", i));
        fs::create_dir(&dir_path).unwrap();
        for j in 0..10 {
            let content = format!("content_{}_{}", i, j);
            fs::write(dir_path.join(format!("file_{}.txt", j)), content).unwrap();
        }
    }

    let start = Instant::now();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .output()
        .expect("Failed to execute parsync");
    let duration = start.elapsed();

    assert!(output.status.success());
    println!("Benchmark: 100 files in 10 directories - Time: {:?}", duration);
}

#[test]
fn benchmark_sync_with_threads_2() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create 100 files
    for i in 0..100 {
        let content = vec![b'x'; 10 * 1024]; // 10KB each
        fs::write(source_dir.path().join(format!("file_{}.bin", i)), content).unwrap();
    }

    let start = Instant::now();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .arg("-t")
        .arg("2")
        .output()
        .expect("Failed to execute parsync");
    let duration = start.elapsed();

    assert!(output.status.success());
    println!("Benchmark: 100 files with 2 threads - Time: {:?}", duration);
}

#[test]
fn benchmark_sync_with_threads_4() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create 100 files
    for i in 0..100 {
        let content = vec![b'x'; 10 * 1024]; // 10KB each
        fs::write(source_dir.path().join(format!("file_{}.bin", i)), content).unwrap();
    }

    let start = Instant::now();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .arg("-t")
        .arg("4")
        .output()
        .expect("Failed to execute parsync");
    let duration = start.elapsed();

    assert!(output.status.success());
    println!("Benchmark: 100 files with 4 threads - Time: {:?}", duration);
}

#[test]
fn benchmark_sync_with_threads_8() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create 100 files
    for i in 0..100 {
        let content = vec![b'x'; 10 * 1024]; // 10KB each
        fs::write(source_dir.path().join(format!("file_{}.bin", i)), content).unwrap();
    }

    let start = Instant::now();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .arg("-t")
        .arg("8")
        .output()
        .expect("Failed to execute parsync");
    let duration = start.elapsed();

    assert!(output.status.success());
    println!("Benchmark: 100 files with 8 threads - Time: {:?}", duration);
}

#[test]
fn benchmark_sync_incremental_no_changes() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create files
    for i in 0..100 {
        let content = vec![b'x'; 10 * 1024];
        fs::write(source_dir.path().join(format!("file_{}.bin", i)), content).unwrap();
    }

    // First sync
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .output()
        .expect("Failed to execute parsync");
    assert!(output.status.success());

    // Second sync (no changes)
    let start = Instant::now();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .output()
        .expect("Failed to execute parsync");
    let duration = start.elapsed();

    assert!(output.status.success());
    println!("Benchmark: Incremental sync (no changes, 100 files) - Time: {:?}", duration);
}

#[test]
fn benchmark_sync_incremental_few_changes() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create files
    for i in 0..100 {
        let content = vec![b'x'; 10 * 1024];
        fs::write(source_dir.path().join(format!("file_{}.bin", i)), content).unwrap();
    }

    // First sync
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .output()
        .expect("Failed to execute parsync");
    assert!(output.status.success());

    // Modify 5 files
    for i in 0..5 {
        let content = vec![b'y'; 10 * 1024];
        fs::write(source_dir.path().join(format!("file_{}.bin", i)), content).unwrap();
    }

    // Second sync (5 changes)
    let start = Instant::now();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .output()
        .expect("Failed to execute parsync");
    let duration = start.elapsed();

    assert!(output.status.success());
    println!("Benchmark: Incremental sync (5 changed files out of 100) - Time: {:?}", duration);
}

#[test]
fn benchmark_sync_with_no_verify() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create 100 files
    for i in 0..100 {
        let content = vec![b'x'; 10 * 1024];
        fs::write(source_dir.path().join(format!("file_{}.bin", i)), content).unwrap();
    }

    let start = Instant::now();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .arg("--no-verify")
        .output()
        .expect("Failed to execute parsync");
    let duration = start.elapsed();

    assert!(output.status.success());
    println!("Benchmark: 100 files with --no-verify - Time: {:?}", duration);
}
