#[derive(Clone)]
pub enum FileKind<Path> {
    File,
    Directory,
    Symlink(Path),
}

#[derive(Clone)]
pub struct FileEntry<Path> {
    pub path: Path,
    pub kind: FileKind<Path>,
    pub size: u64,
}

pub struct FileMetadata {
    pub size: u64,
    pub checksum: Option<String>,
}

