use log::error;
use ssh2::Session;
use std::io::Read;
use std::net::TcpStream;
use std::path::Path;

/// SSH session helper to manage connections and execute commands
pub struct SSHSessionHelper {
    user: String,
    host: String,
}

impl SSHSessionHelper {
    pub fn new(user: String, host: String) -> Self {
        Self { user, host }
    }

    /// Create a new SSH session
    pub fn connect(&self) -> std::io::Result<Session> {
        let tcp = TcpStream::connect(format!("{}:22", self.host))?;
        let mut sess = Session::new().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to create session: {}", e))
        })?;
        sess.set_tcp_stream(tcp);
        sess.handshake().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("SSH handshake failed: {}", e))
        })?;

        // Try to authenticate using SSH agent first, then fall back to key files
        if let Err(e) = sess.userauth_agent(&self.user) {
            error!("SSH agent authentication failed: {}, trying key files", e);
            
            // Try common SSH key locations
            let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/root"));
            let key_paths = vec![
                format!("{}/.ssh/id_rsa", home),
                format!("{}/.ssh/id_ed25519", home),
                format!("{}/.ssh/id_ecdsa", home),
            ];
            
            let mut authenticated = false;
            for key_path in key_paths {
                if Path::new(&key_path).exists() {
                    if let Ok(_) = sess.userauth_pubkey_file(&self.user, None, Path::new(&key_path), None) {
                        authenticated = true;
                        break;
                    }
                }
            }
            
            if !authenticated {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "SSH authentication failed: no valid credentials found"
                ));
            }
        }

        Ok(sess)
    }

    /// Execute a command on the remote host
    pub fn execute_command(&self, command: &str) -> std::io::Result<String> {
        let sess = self.connect()?;
        let mut channel = sess.channel_session().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to open channel: {}", e))
        })?;
        
        channel.exec(command).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to execute command: {}", e))
        })?;
        
        let mut output = String::new();
        channel.read_to_string(&mut output).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to read output: {}", e))
        })?;
        
        channel.wait_close().ok();
        let exit_status = channel.exit_status().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to get exit status: {}", e))
        })?;
        
        if exit_status != 0 {
            let mut stderr = String::new();
            channel.stderr().read_to_string(&mut stderr).ok();
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Command failed with exit status {}: {}", exit_status, stderr)
            ));
        }
        
        Ok(output)
    }

    /// Read a file from the remote host using SFTP
    pub fn read_file(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        let sess = self.connect()?;
        let sftp = sess.sftp().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to start SFTP: {}", e))
        })?;
        
        let mut file = sftp.open(path).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to open file: {}", e))
        })?;
        
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)?;
        Ok(contents)
    }

    /// Write a file to the remote host using SFTP
    pub fn write_file(&self, local_path: &Path, remote_path: &Path) -> std::io::Result<()> {
        let sess = self.connect()?;
        let sftp = sess.sftp().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to start SFTP: {}", e))
        })?;
        
        // Read local file
        let contents = std::fs::read(local_path)?;
        
        // Write to remote file
        let mut remote_file = sftp.create(remote_path).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to create remote file: {}", e))
        })?;
        
        std::io::Write::write_all(&mut remote_file, &contents)?;
        Ok(())
    }

    /// Check if a file or directory exists on the remote host
    pub fn path_exists(&self, path: &Path) -> bool {
        let sess = match self.connect() {
            Ok(s) => s,
            Err(_) => return false,
        };
        
        let sftp = match sess.sftp() {
            Ok(s) => s,
            Err(_) => return false,
        };
        
        sftp.stat(path).is_ok()
    }

    /// Create a directory on the remote host using SFTP
    pub fn create_dir(&self, path: &Path) -> std::io::Result<()> {
        let sess = self.connect()?;
        let sftp = sess.sftp().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to start SFTP: {}", e))
        })?;
        
        // Create directories recursively
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !self.path_exists(parent) {
                self.create_dir(parent)?;
            }
        }
        
        // Try to create the directory, ignore error if it already exists
        if let Err(e) = sftp.mkdir(path, 0o755) {
            // Check if it already exists
            if sftp.stat(path).is_err() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to create directory: {}", e)
                ));
            }
        }
        
        Ok(())
    }
}
