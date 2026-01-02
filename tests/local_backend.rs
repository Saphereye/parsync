use parsync::backends::{LocalBackend, StorageBackend};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn test_localbackend_put_and_get() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("testfile.txt");
    let backend = LocalBackend::new();

    // Write data
    let data = b"hello world";
    backend.put(file_path.to_str().unwrap(), data).unwrap();

    // Read data
    let read = backend.get(file_path.to_str().unwrap()).unwrap();
    assert_eq!(read, data);
}

#[test]
fn test_localbackend_exists() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("exists.txt");
    let backend = LocalBackend::new();

    // Should not exist yet
    assert!(!backend.exists(file_path.to_str().unwrap()).unwrap());

    // Create file
    File::create(&file_path).unwrap();

    // Should exist now
    assert!(backend.exists(file_path.to_str().unwrap()).unwrap());
}

#[test]
/// Verify that LocalBackend::delete removes a single file.
fn test_localbackend_delete_file() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("delete_me.txt");
    let backend = LocalBackend::new();

    // Create file
    {
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "delete me!").unwrap();
    }
    assert!(file_path.exists());

    // Delete file
    backend.delete(file_path.to_str().unwrap()).unwrap();
    assert!(!file_path.exists());
}

#[test]
/// Verify that LocalBackend::delete removes a directory recursively.
fn test_localbackend_delete_directory() {
    let dir = tempdir().unwrap();
    let subdir_path = dir.path().join("subdir");
    let file_path = subdir_path.join("file.txt");
    let backend = LocalBackend::new();

    // Create directory and file
    fs::create_dir(&subdir_path).unwrap();
    File::create(&file_path).unwrap();
    assert!(subdir_path.exists());
    assert!(file_path.exists());

    // Delete directory
    backend.delete(subdir_path.to_str().unwrap()).unwrap();
    assert!(!subdir_path.exists());
    assert!(!file_path.exists());
}

#[test]
fn test_localbackend_list() {
    let dir = tempdir().unwrap();
    let file1 = dir.path().join("a.txt");
    let file2 = dir.path().join("b.txt");
    File::create(&file1).unwrap();
    File::create(&file2).unwrap();
    let backend = LocalBackend::new();

    let mut entries = backend.list(dir.path().to_str().unwrap()).unwrap();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let paths: Vec<_> = entries
        .iter()
        .map(|e| {
            Path::new(&e.path)
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string()
        })
        .collect();

    assert_eq!(paths, vec!["a.txt".to_string(), "b.txt".to_string()]);
}
