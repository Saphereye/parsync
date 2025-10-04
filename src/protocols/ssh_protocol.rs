use crate::{protocols::protocol::Protocol, utils::Status};
use std::path::PathBuf;

/// Legacy SSH protocol implementation - deprecated
/// Use SSHSource and SSHSink instead
#[allow(dead_code)]
pub struct SSHProtocol;

#[allow(dead_code)]
impl SSHProtocol {}

#[allow(unused_variables)]
impl Protocol<PathBuf> for SSHProtocol {
    fn get_file_list(
        source: &PathBuf,
        destination: Option<&PathBuf>,
        include_regex: Option<String>,
        exclude_regex: Option<String>,
        no_verify: bool,
    ) -> Vec<(PathBuf, u64)> {
        todo!()
    }

    fn sync_files(
        files: &Vec<(PathBuf, u64)>,
        source: &PathBuf,
        destination: &PathBuf,
        pb: &Option<indicatif::ProgressBar>,
        dry_run: bool,
    ) {
        todo!()
    }

    fn compare_dirs(src: &PathBuf, dest: &PathBuf) -> Status {
        todo!()
    }

    fn compare_file_metadata(src: &PathBuf, dest: &PathBuf, file: &PathBuf) -> Status {
        todo!()
    }

    fn file_checksum(path: &PathBuf) -> Option<String> {
        todo!()
    }

    fn create_symlink(target: &PathBuf, link: &PathBuf) -> std::io::Result<()> {
        todo!()
    }
}
