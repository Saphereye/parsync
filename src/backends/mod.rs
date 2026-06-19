pub mod local;
pub mod ssh;

use std::sync::Arc;

#[derive(Debug)]
pub enum SyncError {
    Io(std::io::Error),
    NotFound(String),
    Other(String),
}

impl From<std::io::Error> for SyncError {
    fn from(e: std::io::Error) -> Self {
        SyncError::Io(e)
    }
}

#[derive(Debug, Clone)]
pub struct FileMeta {
    pub size: u64,
    pub is_dir: bool,
    pub modified: Option<std::time::SystemTime>,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub metadata: FileMeta,
}

pub trait StorageBackend: Send + Sync + std::any::Any {
    fn list(&self, path: &str) -> Result<Vec<FileEntry>, SyncError>;
    fn get(&self, path: &str) -> Result<Vec<u8>, SyncError>;
    fn put(&self, path: &str, data: &[u8]) -> Result<(), SyncError>;
    fn delete(&self, path: &str) -> Result<(), SyncError>;
    fn exists(&self, path: &str) -> Result<bool, SyncError>;
    fn as_any(&self) -> &dyn std::any::Any;

    fn put_stream(
        &self,
        path: &str,
        reader: &mut dyn std::io::Read,
        size: u64,
    ) -> Result<(), SyncError> {
        let mut data = Vec::with_capacity(size as usize);
        reader.read_to_end(&mut data)?;
        self.put(path, &data)
    }
}

pub use local::LocalBackend;
pub use ssh::SshBackend;

pub fn backend_and_path(
    url: &str,
    pool_size: usize,
) -> Result<(Arc<dyn StorageBackend + Send + Sync>, &str), SyncError> {
    if let Some(idx) = url.find("://") {
        let (proto, rest) = url.split_at(idx);
        let after_scheme = &rest[3..];
        match proto {
            "file" => Ok((Arc::new(LocalBackend::new()), after_scheme)),
            "ssh" => {
                let (user, host_port_path) = if let Some(at) = after_scheme.find('@') {
                    (after_scheme[..at].to_string(), &after_scheme[at + 1..])
                } else {
                    (
                        std::env::var("USER").unwrap_or_else(|_| "root".to_string()),
                        after_scheme,
                    )
                };

                let slash = host_port_path
                    .find('/')
                    .ok_or_else(|| SyncError::Other(format!("SSH URI missing path: {url}")))?;
                let host_port = &host_port_path[..slash];
                let remote_path = &host_port_path[slash..];

                let (host, port) = if let Some(colon) = host_port.rfind(':') {
                    let p = host_port[colon + 1..].parse::<u16>().unwrap_or(22);
                    (&host_port[..colon], p)
                } else {
                    (host_port, 22u16)
                };

                let backend = SshBackend::connect(&user, host, port, pool_size)?;
                Ok((Arc::new(backend), remote_path))
            }
            _ => Err(SyncError::Other(format!("Unsupported protocol: {proto}"))),
        }
    } else {
        Ok((Arc::new(LocalBackend::new()), url))
    }
}
