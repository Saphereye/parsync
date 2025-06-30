pub mod traits;
pub mod metadata;
pub mod local_protocol;
pub mod ssh_protocol;

use std::sync::{atomic::{AtomicU64, Ordering}, Arc};

use metadata::FileEntry;
pub use traits::{Sink, Source};
pub use local_protocol::LocalProtocol;

pub fn sync<S, D>(
    src: &S,
    dst: &D,
    files: Vec<FileEntry<S::Path>>,
    is_dry_run: bool,
    pb: &Option<indicatif::ProgressBar>,
) -> anyhow::Result<()>
where
    S: Source,
    D: Sink,
    S::Path: Into<D::Path>,
{
    for file in files {
        let src_path = &file.path;
        let dst_path: D::Path = src_path.clone().into();

        let metadata = src.get_metadata(src_path)?;

        if dst.file_exists(&dst_path)? && dst.compare_metadata(&dst_path, &metadata) {
            continue;
        }

        let mut reader = src.read_file(src_path)?;
        if !is_dry_run {
            dst.write_file(&dst_path, &mut reader, metadata.size)?;
        }

        if let Some(pb) = pb {
            pb.inc(metadata.size);
            pb.tick();
        }

    }

    Ok(())
}

pub trait Protocol: Source + Sink {}
