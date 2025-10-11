use std::fs;
use tempfile::TempDir;

/// Integration tests for local to local file synchronization

#[test]
fn test_sync_empty_directory() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .output()
        .expect("Failed to execute parsync");

    assert!(output.status.success());
}

#[test]
fn test_sync_single_file() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create a test file
    let test_file = source_dir.path().join("test.txt");
    fs::write(&test_file, "test content").unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .output()
        .expect("Failed to execute parsync");

    assert!(output.status.success());

    // Verify file was copied
    let dest_file = dest_dir.path().join("test.txt");
    assert!(dest_file.exists());
    let content = fs::read_to_string(dest_file).unwrap();
    assert_eq!(content, "test content");
}

#[test]
fn test_sync_multiple_files() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create multiple test files
    for i in 0..10 {
        let test_file = source_dir.path().join(format!("test_{}.txt", i));
        fs::write(&test_file, format!("content {}", i)).unwrap();
    }

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .output()
        .expect("Failed to execute parsync");

    assert!(output.status.success());

    // Verify all files were copied
    for i in 0..10 {
        let dest_file = dest_dir.path().join(format!("test_{}.txt", i));
        assert!(dest_file.exists());
        let content = fs::read_to_string(dest_file).unwrap();
        assert_eq!(content, format!("content {}", i));
    }
}

#[test]
fn test_sync_nested_directories() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create nested directory structure
    let nested_path = source_dir.path().join("dir1").join("dir2").join("dir3");
    fs::create_dir_all(&nested_path).unwrap();
    fs::write(nested_path.join("test.txt"), "nested content").unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .output()
        .expect("Failed to execute parsync");

    assert!(output.status.success());

    // Verify nested structure was copied
    let dest_file = dest_dir.path().join("dir1").join("dir2").join("dir3").join("test.txt");
    assert!(dest_file.exists());
    let content = fs::read_to_string(dest_file).unwrap();
    assert_eq!(content, "nested content");
}

#[test]
fn test_sync_with_include_filter() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create files with different extensions
    fs::write(source_dir.path().join("file1.txt"), "text content").unwrap();
    fs::write(source_dir.path().join("file2.log"), "log content").unwrap();
    fs::write(source_dir.path().join("file3.txt"), "more text").unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .arg("-i")
        .arg(r"\.txt$")
        .output()
        .expect("Failed to execute parsync");

    assert!(output.status.success());

    // Verify only .txt files were copied
    assert!(dest_dir.path().join("file1.txt").exists());
    assert!(!dest_dir.path().join("file2.log").exists());
    assert!(dest_dir.path().join("file3.txt").exists());
}

#[test]
fn test_sync_with_exclude_filter() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create files with different extensions
    fs::write(source_dir.path().join("file1.txt"), "text content").unwrap();
    fs::write(source_dir.path().join("file2.log"), "log content").unwrap();
    fs::write(source_dir.path().join("file3.txt"), "more text").unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .arg("-e")
        .arg(r"\.log$")
        .output()
        .expect("Failed to execute parsync");

    assert!(output.status.success());

    // Verify .log file was excluded
    assert!(dest_dir.path().join("file1.txt").exists());
    assert!(!dest_dir.path().join("file2.log").exists());
    assert!(dest_dir.path().join("file3.txt").exists());
}

#[test]
fn test_sync_dry_run() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create a test file
    fs::write(source_dir.path().join("test.txt"), "test content").unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .arg("--dry-run")
        .output()
        .expect("Failed to execute parsync");

    assert!(output.status.success());

    // Verify file was NOT copied (dry run)
    assert!(!dest_dir.path().join("test.txt").exists());
}

#[test]
fn test_sync_no_verify() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create test files
    fs::write(source_dir.path().join("test.txt"), "test content").unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .arg("--no-verify")
        .output()
        .expect("Failed to execute parsync");

    assert!(output.status.success());

    // Verify file was copied
    let dest_file = dest_dir.path().join("test.txt");
    assert!(dest_file.exists());
}

#[test]
fn test_sync_incremental() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // First sync
    fs::write(source_dir.path().join("file1.txt"), "content1").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .output()
        .expect("Failed to execute parsync");
    assert!(output.status.success());

    // Add new file and sync again
    fs::write(source_dir.path().join("file2.txt"), "content2").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .output()
        .expect("Failed to execute parsync");
    assert!(output.status.success());

    // Verify both files exist
    assert!(dest_dir.path().join("file1.txt").exists());
    assert!(dest_dir.path().join("file2.txt").exists());
}

#[test]
fn test_sync_large_file() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create a large file (1MB)
    let large_content = vec![b'A'; 1024 * 1024];
    fs::write(source_dir.path().join("large.bin"), &large_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .output()
        .expect("Failed to execute parsync");

    assert!(output.status.success());

    // Verify file was copied correctly
    let dest_file = dest_dir.path().join("large.bin");
    assert!(dest_file.exists());
    let dest_content = fs::read(dest_file).unwrap();
    assert_eq!(dest_content.len(), large_content.len());
}

#[test]
fn test_sync_binary_files() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    // Create binary files with various byte patterns
    let binary_data = vec![0u8, 1, 2, 3, 255, 254, 128, 127];
    fs::write(source_dir.path().join("binary.dat"), &binary_data).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .output()
        .expect("Failed to execute parsync");

    assert!(output.status.success());

    // Verify binary file integrity
    let dest_file = dest_dir.path().join("binary.dat");
    assert!(dest_file.exists());
    let dest_data = fs::read(dest_file).unwrap();
    assert_eq!(dest_data, binary_data);
}

#[test]
fn test_sync_preserves_file_content() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();

    let original_content = "This is a test file with\nmultiple lines\nand special chars: !@#$%^&*()";
    fs::write(source_dir.path().join("test.txt"), original_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parsync"))
        .arg("-s")
        .arg(source_dir.path())
        .arg("-d")
        .arg(dest_dir.path())
        .output()
        .expect("Failed to execute parsync");

    assert!(output.status.success());

    let dest_content = fs::read_to_string(dest_dir.path().join("test.txt")).unwrap();
    assert_eq!(dest_content, original_content);
}
