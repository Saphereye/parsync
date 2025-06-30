use crate::protocols::metadata::{FileEntry, FileMetadata};

pub trait Source {
    type Path: Clone + Send + Sync + 'static;

    fn list_files(
        &self,
        base: &Self::Path,
        include: Option<&str>,
        exclude: Option<&str>,
    ) -> anyhow::Result<Vec<(FileEntry<Self::Path>, u64)>>;

    fn read_file(&self, path: &Self::Path) -> anyhow::Result<Box<dyn std::io::Read + Send>>;

    fn get_metadata(&self, path: &Self::Path) -> anyhow::Result<FileMetadata>;
}

pub trait Sink {
    type Path: Clone + Send + Sync + 'static;

    fn write_file(
        &self,
        path: &Self::Path,
        reader: &mut dyn std::io::Read,
        expected_size: u64,
    ) -> anyhow::Result<()>;

    fn create_symlink(&self, target: &Self::Path, link: &Self::Path) -> std::io::Result<()>;

    fn create_dir(&self, path: &Self::Path) -> std::io::Result<()>;

    fn file_exists(&self, path: &Self::Path) -> anyhow::Result<bool>;

    fn compare_metadata(&self, path: &Self::Path, metadata: &FileMetadata) -> bool;
}
