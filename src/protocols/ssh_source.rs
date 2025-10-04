use crate::protocols::source::Source;
use blake3::Hasher;
use log::error;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// SSH-based source implementation
/// Format: user@host:path
pub struct SSHSource {
    user: String,
    host: String,
    root: PathBuf,
}

impl SSHSource {
    pub fn new(connection_string: &str) -> Result<Self, String> {
        // Parse user@host:path format
        let parts: Vec<&str> = connection_string.split('@').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid SSH connection string: {}", connection_string));
        }
        
        let user = parts[0].to_string();
        let host_path: Vec<&str> = parts[1].split(':').collect();
        if host_path.len() != 2 {
            return Err(format!("Invalid SSH connection string: {}", connection_string));
        }
        
        let host = host_path[0].to_string();
        let root = PathBuf::from(host_path[1]);
        
        Ok(Self { user, host, root })
    }

    pub fn connection_string(&self) -> String {
        format!("{}@{}", self.user, self.host)
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    /// Execute a command on the remote host via SSH
    fn ssh_command(&self, command: &str) -> Result<String, std::io::Error> {
        let output = Command::new("ssh")
            .arg(&self.connection_string())
            .arg(command)
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("SSH command failed: {}", String::from_utf8_lossy(&output.stderr)),
            ))
        }
    }
}

impl Source for SSHSource {
    fn list_files(
        &self,
        _include_regex: Option<String>,
        _exclude_regex: Option<String>,
    ) -> Vec<(PathBuf, u64)> {
        // This method is not used in the new design
        vec![]
    }

    fn get_file_hash(&self, path: &PathBuf) -> Option<String> {
        // Use blake3sum or fallback to computing hash locally
        let path_str = path.to_string_lossy();
        
        // Try to compute hash on remote side using a shell command
        let command = format!(
            "if command -v b3sum >/dev/null 2>&1; then b3sum '{}' | cut -d' ' -f1; else echo 'NO_B3SUM'; fi",
            path_str
        );
        
        match self.ssh_command(&command) {
            Ok(output) => {
                let hash = output.trim();
                if hash == "NO_B3SUM" || hash.is_empty() {
                    // Fallback: read file and compute hash locally
                    match self.read_file(path) {
                        Ok(content) => {
                            let mut hasher = Hasher::new();
                            hasher.update(&content);
                            Some(hasher.finalize().to_hex().to_string())
                        }
                        Err(e) => {
                            error!("Failed to read file {:?}: {}", path, e);
                            None
                        }
                    }
                } else {
                    Some(hash.to_string())
                }
            }
            Err(e) => {
                error!("Failed to get hash for {:?}: {}", path, e);
                None
            }
        }
    }

    fn get_file_hashes(&self, paths: &[PathBuf]) -> HashMap<PathBuf, String> {
        use rayon::prelude::*;
        
        paths
            .par_iter()
            .filter_map(|path| {
                self.get_file_hash(path).map(|hash| (path.clone(), hash))
            })
            .collect()
    }

    fn read_file(&self, path: &PathBuf) -> std::io::Result<Vec<u8>> {
        // Use ssh to read the file
        let output = Command::new("ssh")
            .arg(&self.connection_string())
            .arg("cat")
            .arg(path.to_string_lossy().as_ref())
            .stdout(Stdio::piped())
            .output()?;

        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to read file: {}", String::from_utf8_lossy(&output.stderr)),
            ))
        }
    }

    fn is_symlink(&self, path: &PathBuf) -> bool {
        let command = format!("test -L '{}' && echo 'true' || echo 'false'", path.to_string_lossy());
        
        match self.ssh_command(&command) {
            Ok(output) => output.trim() == "true",
            Err(_) => false,
        }
    }

    fn read_link(&self, path: &PathBuf) -> std::io::Result<PathBuf> {
        let command = format!("readlink '{}'", path.to_string_lossy());
        
        match self.ssh_command(&command) {
            Ok(output) => Ok(PathBuf::from(output.trim())),
            Err(e) => Err(e),
        }
    }
}
