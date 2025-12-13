use crate::utils::Status;
use indicatif::ProgressBar;

pub trait Protocol<PathType>
where
    PathType: Sized,
{
    fn get_file_list(
        source: &PathType,
        destination: Option<&PathType>,
        include_regex: Option<String>,
        exclude_regex: Option<String>,
        no_verify: bool,
    ) -> Vec<(PathType, u64)>;

    fn sync_files(
        files: &Vec<(PathType, u64)>,
        source: &PathType,
        destination: &PathType,
        pb: &Option<ProgressBar>,
        dry_run: bool,
    );

    fn compare_dirs(src: &PathType, dest: &PathType) -> Status;

    fn compare_file_metadata(src: &PathType, dest: &PathType, file: &PathType) -> Status;

    fn file_checksum(path: &PathType) -> Option<String>;

    fn create_symlink(target: &PathType, link: &PathType) -> std::io::Result<()>;
}
