use std::ops::Not;

#[derive(Debug, PartialEq)]
pub enum Status {
    Passed,
    Failed,
}

impl Not for Status {
    type Output = Status;

    fn not(self) -> Self::Output {
        match self {
            Status::Passed => Status::Failed,
            Status::Failed => Status::Passed,
        }
    }
}

pub fn size_to_human_readable(size: f64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let mut size = size;
    let mut unit = 0;
    while size >= 1024.0 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.2} {}", size, units[unit])
}
