/// Convert byte size to human-readable format
pub fn size_to_human_readable(size: f64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let mut size = size;
    let mut unit = 0;
    while size >= 1024.0 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.2} {}", size, units[unit])
}

/// Compute Blake3 hash for a local file
pub fn compute_file_hash(path: &std::path::PathBuf) -> Option<String> {
    use blake3::Hasher;
    use std::fs::File;
    use std::io::Read;

    let mut file = File::open(path).ok()?;
    let mut hasher = Hasher::new();
    let mut buffer = [0; 8192];

    while let Ok(n) = file.read(&mut buffer) {
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Some(hasher.finalize().to_hex().to_string())
}
