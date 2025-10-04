use crate::protocols::sink::Sink;
use blake3::Hasher;
use log::error;
use std::path::PathBuf;
use std::process::Command;

/// SSH-based sink implementation
/// 
/// Format: user@host:path
pub struct SSHSink {
    user: String,
    host: String,
    root: PathBuf,
}

impl SSHSink {
    /// Parse and create SSH sink from connection string (user@host:path)
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

impl Sink for SSHSink {
    fn file_exists(&self, path: &PathBuf) -> bool {
        let command = format!("test -e '{}' && echo 'true' || echo 'false'", path.to_string_lossy());
        
        match self.ssh_command(&command) {
            Ok(output) => output.trim() == "true",
            Err(_) => false,
        }
    }

    fn get_file_hash(&self, path: &PathBuf) -> Option<String> {
        let path_str = path.to_string_lossy();
        
        // Try to compute hash on remote side
        let command = format!(
            "if command -v b3sum >/dev/null 2>&1; then b3sum '{}' | cut -d' ' -f1; else echo 'NO_B3SUM'; fi",
            path_str
        );
        
        match self.ssh_command(&command) {
            Ok(output) => {
                let hash = output.trim();
                if hash == "NO_B3SUM" || hash.is_empty() {
                    // Fallback: read file via SSH and compute hash locally
                    let read_command = format!("cat '{}'", path_str);
                    match self.ssh_command(&read_command) {
                        Ok(content) => {
                            let mut hasher = Hasher::new();
                            hasher.update(content.as_bytes());
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

    fn create_dir(&self, path: &PathBuf) -> std::io::Result<()> {
        let command = format!("mkdir -p '{}'", path.to_string_lossy());
        self.ssh_command(&command)?;
        Ok(())
    }

    fn create_symlink(&self, target: &PathBuf, link: &PathBuf) -> std::io::Result<()> {
        // Create parent directory first
        if let Some(parent) = link.parent() {
            self.create_dir(&parent.to_path_buf())?;
        }

        let command = format!(
            "ln -s '{}' '{}'",
            target.to_string_lossy(),
            link.to_string_lossy()
        );
        self.ssh_command(&command)?;
        Ok(())
    }

    fn copy_file(&self, source_path: &PathBuf, dest_path: &PathBuf) -> std::io::Result<()> {
        // Create parent directory first
        if let Some(parent) = dest_path.parent() {
            self.create_dir(&parent.to_path_buf())?;
        }

        // Use scp to copy the file
        let remote_dest = format!("{}:{}", self.connection_string(), dest_path.to_string_lossy());
        
        let output = Command::new("scp")
            .arg("-q")
            .arg(source_path)
            .arg(&remote_dest)
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("SCP failed: {}", String::from_utf8_lossy(&output.stderr)),
            ))
        }
    }
}
