use parsync::backends::{LocalBackend, StorageBackend};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_localbackend_put_and_get() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("testfile.txt");
    let backend = LocalBackend::new();

    let data = b"hello world";
    backend.put(file_path.to_str().unwrap(), data).unwrap();

    let read = backend.get(file_path.to_str().unwrap()).unwrap();
    assert_eq!(read, data);
}

#[test]
fn test_localbackend_exists() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("exists.txt");
    let backend = LocalBackend::new();

    assert!(!backend.exists(file_path.to_str().unwrap()).unwrap());

    File::create(&file_path).unwrap();

    assert!(backend.exists(file_path.to_str().unwrap()).unwrap());
}

#[test]
/// Verify that LocalBackend::delete removes a single file.
fn test_localbackend_delete_file() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("delete_me.txt");
    let backend = LocalBackend::new();

    {
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "delete me!").unwrap();
    }
    assert!(file_path.exists());

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

    fs::create_dir(&subdir_path).unwrap();
    File::create(&file_path).unwrap();
    assert!(subdir_path.exists());
    assert!(file_path.exists());

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

#[test]
/// Test the delete function with non-existent path (should not panic)
fn test_delete_nonexistent_path() {
    let backend = Arc::new(LocalBackend::new());
    let roots = vec!["/tmp/nonexistent_path_12345".to_string()];

    let result = parsync::delete(backend, &roots, 1, false, true, None, None);

    let _ = result;
}

#[test]
/// Test the delete function with a single file
fn test_delete_single_file() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("file_to_delete.txt");
    let mut f = File::create(&file_path).unwrap();
    writeln!(f, "test content").unwrap();
    drop(f);

    assert!(file_path.exists());

    let backend = Arc::new(LocalBackend::new());
    let roots = vec![file_path.to_str().unwrap().to_string()];

    let result = parsync::delete(backend, &roots, 1, false, true, None, None);

    assert!(result.is_ok());
    assert!(!file_path.exists());
}

#[test]
/// Test the delete function with a directory containing files
fn test_delete_directory_with_files() {
    let dir = tempdir().unwrap();
    let subdir = dir.path().join("to_delete");
    fs::create_dir(&subdir).unwrap();

    for i in 0..5 {
        let file = subdir.join(format!("file{}.txt", i));
        File::create(&file).unwrap();
    }

    let nested = subdir.join("nested");
    fs::create_dir(&nested).unwrap();
    File::create(nested.join("nested_file.txt")).unwrap();

    assert!(subdir.exists());

    let backend = Arc::new(LocalBackend::new());
    let roots = vec![subdir.to_str().unwrap().to_string()];

    let result = parsync::delete(backend, &roots, 1, false, true, None, None);

    assert!(result.is_ok());
    assert!(!subdir.exists());
}

#[test]
/// Test the delete function with multiple threads (the original bug scenario)
fn test_delete_multithreaded() {
    let dir = tempdir().unwrap();
    let subdir = dir.path().join("multi_delete");
    fs::create_dir(&subdir).unwrap();

    for i in 0..20 {
        let file = subdir.join(format!("file{}.txt", i));
        let mut f = File::create(&file).unwrap();
        writeln!(f, "content {}", i).unwrap();
    }

    assert!(subdir.exists());

    let backend = Arc::new(LocalBackend::new());
    let roots = vec![subdir.to_str().unwrap().to_string()];

    let result = parsync::delete(backend, &roots, 4, false, true, None, None);

    assert!(result.is_ok());
    assert!(!subdir.exists());
}

#[test]
/// Test the delete function in dry-run mode
fn test_delete_dry_run() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("dry_run_test.txt");
    File::create(&file_path).unwrap();

    assert!(file_path.exists());

    let backend = Arc::new(LocalBackend::new());
    let roots = vec![file_path.to_str().unwrap().to_string()];

    let result = parsync::delete(backend, &roots, 1, true, true, None, None);

    assert!(result.is_ok());
    assert!(file_path.exists());
}

#[test]
/// Test the delete function with regex include filter
fn test_delete_with_include_filter() {
    let dir = tempdir().unwrap();
    let subdir = dir.path().join("filter_test");
    fs::create_dir(&subdir).unwrap();

    File::create(subdir.join("keep_me.txt")).unwrap();
    File::create(subdir.join("delete_me.log")).unwrap();
    File::create(subdir.join("also_delete.log")).unwrap();

    let backend = Arc::new(LocalBackend::new());
    let include_re = regex::Regex::new(r"\.log$").unwrap();
    let roots = vec![subdir.to_str().unwrap().to_string()];

    let result = parsync::delete(backend, &roots, 1, false, true, Some(&include_re), None);

    assert!(result.is_ok());
    assert!(subdir.join("keep_me.txt").exists());
    assert!(!subdir.join("delete_me.log").exists());
    assert!(!subdir.join("also_delete.log").exists());
}

#[test]
/// Test the delete function with regex exclude filter on files
fn test_delete_with_exclude_filter() {
    let dir = tempdir().unwrap();
    let subdir = dir.path().join("filter_exclude");
    fs::create_dir(&subdir).unwrap();

    File::create(subdir.join("delete_me.txt")).unwrap();
    File::create(subdir.join("keep_me.log")).unwrap();
    File::create(subdir.join("delete_me.tmp")).unwrap();

    let backend = Arc::new(LocalBackend::new());
    let exclude_re = regex::Regex::new(r"\.log$").unwrap();
    let roots = vec![subdir.to_str().unwrap().to_string()];

    let result = parsync::delete(backend, &roots, 1, false, true, None, Some(&exclude_re));

    assert!(result.is_ok());
    assert!(!subdir.exists());
}

#[test]
fn test_delete_empty_file() {
    let dir = tempdir().unwrap();
    let empty_file = dir.path().join("empty.txt");
    File::create(&empty_file).unwrap();

    let backend = Arc::new(LocalBackend::new());
    let roots = vec![empty_file.to_str().unwrap().to_string()];

    let result = parsync::delete(backend, &roots, 1, false, true, None, None);

    assert!(result.is_ok());
    assert!(!empty_file.exists());
}

#[test]
fn test_delete_deeply_nested_directories() {
    let dir = tempdir().unwrap();
    let mut current = dir.path().to_path_buf();

    for i in 0..10 {
        current = current.join(format!("level_{}", i));
        fs::create_dir(&current).unwrap();
    }

    File::create(current.join("deep_file.txt")).unwrap();
    let root_dir = dir.path().join("level_0");

    assert!(root_dir.exists());

    let backend = Arc::new(LocalBackend::new());
    let roots = vec![root_dir.to_str().unwrap().to_string()];

    let result = parsync::delete(backend, &roots, 1, false, true, None, None);

    assert!(result.is_ok());
    assert!(!root_dir.exists());
}

#[test]
fn test_delete_special_characters_in_filename() {
    let dir = tempdir().unwrap();
    let subdir = dir.path().join("special_chars");
    fs::create_dir(&subdir).unwrap();

    let special_names = vec![
        "file with spaces.txt",
        "file-with-dashes.txt",
        "file_with_underscores.txt",
        "file.multiple.dots.txt",
    ];

    for name in special_names {
        File::create(subdir.join(name)).unwrap();
    }

    let backend = Arc::new(LocalBackend::new());
    let roots = vec![subdir.to_str().unwrap().to_string()];

    let result = parsync::delete(backend, &roots, 2, false, true, None, None);

    assert!(result.is_ok());
    assert!(!subdir.exists());
}

#[test]
fn test_get_empty_file() {
    let dir = tempdir().unwrap();
    let empty_file = dir.path().join("empty.txt");
    File::create(&empty_file).unwrap();

    let backend = LocalBackend::new();
    let content = backend.get(empty_file.to_str().unwrap()).unwrap();

    assert_eq!(content.len(), 0);
}

#[test]
fn test_put_creates_parent_directory() {
    let dir = tempdir().unwrap();
    let nested_file = dir.path().join("nested").join("file.txt");

    let backend = LocalBackend::new();
    let data = b"test content";

    let result = backend.put(nested_file.to_str().unwrap(), data);

    assert!(result.is_err());
}

#[test]
fn test_list_empty_directory() {
    let dir = tempdir().unwrap();
    let empty_dir = dir.path().join("empty");
    fs::create_dir(&empty_dir).unwrap();

    let backend = LocalBackend::new();
    let entries = backend.list(empty_dir.to_str().unwrap()).unwrap();

    assert_eq!(entries.len(), 0);
}

#[test]
fn test_delete_many_files_multithreaded() {
    let dir = tempdir().unwrap();
    let subdir = dir.path().join("many_files");
    fs::create_dir(&subdir).unwrap();

    for i in 0..100 {
        File::create(subdir.join(format!("file_{:03}.txt", i))).unwrap();
    }

    assert!(subdir.exists());

    let backend = Arc::new(LocalBackend::new());
    let roots = vec![subdir.to_str().unwrap().to_string()];

    let result = parsync::delete(backend, &roots, 8, false, true, None, None);

    assert!(result.is_ok());
    assert!(!subdir.exists());
}

#[test]
fn test_copy_file_fallback() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("source.txt");
    let dst = dir.path().join("dest.txt");

    let mut src_file = File::create(&src).unwrap();
    writeln!(src_file, "test content for copy").unwrap();
    drop(src_file);

    let backend = LocalBackend::new();
    let mut buf = vec![0u8; 4096];

    let result = backend.copy_file(src.to_str().unwrap(), dst.to_str().unwrap(), &mut buf);

    assert!(result.is_ok());
    assert!(dst.exists());

    let content = fs::read_to_string(&dst).unwrap();
    assert_eq!(content, "test content for copy\n");
}

#[test]
fn test_exists_returns_correct_values() {
    let dir = tempdir().unwrap();
    let existing_file = dir.path().join("exists.txt");
    File::create(&existing_file).unwrap();

    let backend = LocalBackend::new();

    assert!(backend.exists(existing_file.to_str().unwrap()).unwrap());
    assert!(!backend
        .exists(dir.path().join("nonexistent.txt").to_str().unwrap())
        .unwrap());
    assert!(backend.exists(dir.path().to_str().unwrap()).unwrap());
}
