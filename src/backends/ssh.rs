use crossbeam_channel as channel;
use ssh2::Session;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use super::{FileEntry, FileMeta, StorageBackend, SyncError};

const CHUNK: usize = 1 << 20;

struct SftpConn {
    sftp: ssh2::Sftp,
    mkdirs: HashSet<String>,
    _session: Session,
}

unsafe impl Send for SftpConn {}

struct Pool {
    idle: channel::Receiver<SftpConn>,
    ret: channel::Sender<SftpConn>,
}

impl Pool {
    fn new(conns: Vec<SftpConn>) -> Self {
        let (tx, rx) = channel::bounded(conns.len());
        for c in conns {
            tx.send(c).unwrap();
        }
        Self { idle: rx, ret: tx }
    }

    fn checkout(&self) -> PoolGuard<'_> {
        PoolGuard {
            conn: Some(self.idle.recv().expect("pool closed")),
            ret: &self.ret,
        }
    }
}

struct PoolGuard<'p> {
    conn: Option<SftpConn>,
    ret: &'p channel::Sender<SftpConn>,
}

impl PoolGuard<'_> {
    fn sftp(&self) -> &ssh2::Sftp {
        &self.conn.as_ref().unwrap().sftp
    }

    fn ensure_dir(&mut self, path: &Path) {
        let c = self.conn.as_mut().unwrap();
        let key = path.to_string_lossy().to_string();
        if !c.mkdirs.contains(&key) {
            sftp_mkdir_p(&c.sftp, path);
            c.mkdirs.insert(key);
        }
    }
}

impl Drop for PoolGuard<'_> {
    fn drop(&mut self) {
        if let Some(c) = self.conn.take() {
            let _ = self.ret.send(c);
        }
    }
}

pub struct SshBackend {
    pool: Arc<Pool>,
}

fn connect_one(user: &str, host: &str, port: u16) -> Result<SftpConn, SyncError> {
    let tcp = TcpStream::connect(format!("{host}:{port}"))
        .map_err(|e| SyncError::Other(format!("TCP {host}:{port}: {e}")))?;
    let mut sess = Session::new().map_err(|e| SyncError::Other(format!("SSH session: {e}")))?;
    sess.set_tcp_stream(tcp);
    sess.handshake()
        .map_err(|e| SyncError::Other(format!("SSH handshake: {e}")))?;

    if sess.userauth_agent(user).is_err() {
        let home = std::env::var("HOME").unwrap_or_default();
        let keys = [
            format!("{home}/.ssh/id_ed25519"),
            format!("{home}/.ssh/id_ecdsa"),
            format!("{home}/.ssh/id_rsa"),
        ];
        let ok = keys.iter().any(|k| {
            Path::new(k).exists()
                && sess
                    .userauth_pubkey_file(
                        user,
                        Some(Path::new(&format!("{k}.pub"))),
                        Path::new(k),
                        None,
                    )
                    .is_ok()
        });
        if !ok {
            return Err(SyncError::Other(format!(
                "SSH auth failed for {user}@{host}"
            )));
        }
    }

    let sftp = sess
        .sftp()
        .map_err(|e| SyncError::Other(format!("SFTP init: {e}")))?;
    Ok(SftpConn {
        sftp,
        mkdirs: HashSet::new(),
        _session: sess,
    })
}

impl SshBackend {
    pub fn connect(user: &str, host: &str, port: u16, pool_size: usize) -> Result<Self, SyncError> {
        let conns: Result<Vec<_>, _> = (0..pool_size.max(1))
            .map(|_| connect_one(user, host, port))
            .collect();
        Ok(Self {
            pool: Arc::new(Pool::new(conns?)),
        })
    }
}

fn sftp_mkdir_p(sftp: &ssh2::Sftp, path: &Path) {
    for ancestor in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
        if ancestor.as_os_str().is_empty() || ancestor == Path::new("/") {
            continue;
        }
        let _ = sftp.mkdir(ancestor, 0o755);
    }
}

impl StorageBackend for SshBackend {
    fn list(&self, path: &str) -> Result<Vec<FileEntry>, SyncError> {
        let guard = self.pool.checkout();
        let entries = guard
            .sftp()
            .readdir(Path::new(path))
            .map_err(|e| SyncError::Other(format!("SFTP readdir {path}: {e}")))?;
        Ok(entries
            .into_iter()
            .map(|(p, stat)| FileEntry {
                path: p.to_string_lossy().to_string(),
                metadata: FileMeta {
                    size: stat.size.unwrap_or(0),
                    is_dir: stat.file_type().is_dir(),
                    modified: stat.mtime.map(|t| UNIX_EPOCH + Duration::from_secs(t)),
                },
            })
            .collect())
    }

    fn get(&self, path: &str) -> Result<Vec<u8>, SyncError> {
        let guard = self.pool.checkout();
        let mut file = guard
            .sftp()
            .open(Path::new(path))
            .map_err(|e| SyncError::Other(format!("SFTP open {path}: {e}")))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn put(&self, path: &str, data: &[u8]) -> Result<(), SyncError> {
        self.put_stream(path, &mut std::io::Cursor::new(data), data.len() as u64)
    }

    fn put_stream(&self, path: &str, reader: &mut dyn Read, _size: u64) -> Result<(), SyncError> {
        let mut guard = self.pool.checkout();
        let p = Path::new(path);
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() {
                guard.ensure_dir(parent);
            }
        }
        let mut remote = guard
            .sftp()
            .create(p)
            .map_err(|e| SyncError::Other(format!("SFTP create {path}: {e}")))?;
        let mut buf = vec![0u8; CHUNK];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            remote.write_all(&buf[..n])?;
        }
        Ok(())
    }

    fn delete(&self, path: &str) -> Result<(), SyncError> {
        let guard = self.pool.checkout();
        let p = Path::new(path);
        if guard.sftp().unlink(p).is_err() {
            guard
                .sftp()
                .rmdir(p)
                .map_err(|e| SyncError::Other(format!("SFTP rmdir {path}: {e}")))?;
        }
        Ok(())
    }

    fn exists(&self, path: &str) -> Result<bool, SyncError> {
        let guard = self.pool.checkout();
        Ok(guard.sftp().stat(Path::new(path)).is_ok())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
