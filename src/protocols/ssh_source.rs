use crate::protocols::source::Source;
use crate::protocols::ssh_session::SSHSessionHelper;
use blake3::Hasher;
use log::error;
use std::path::PathBuf;

/// SSH-based source implementation
/// 
/// Format: user@host:path
pub struct SSHSource {
    user: String,
    host: String,
    root: PathBuf,
    session_helper: SSHSessionHelper,
}

impl SSHSource {
    /// Parse and create SSH source from connection string (user@host:path)
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
        
        let session_helper = SSHSessionHelper::new(user.clone(), host.clone());
        
        Ok(Self { user, host, root, session_helper })
    }

    pub fn connection_string(&self) -> String {
        format!("{}@{}", self.user, self.host)
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    /// Execute a command on the remote host via SSH
    fn ssh_command(&self, command: &str) -> Result<String, std::io::Error> {
        self.session_helper.execute_command(command)
    }
}

impl Source for SSHSource {
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

    fn read_file(&self, path: &PathBuf) -> std::io::Result<Vec<u8>> {
        // Use SFTP to read the file
        self.session_helper.read_file(path)
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
