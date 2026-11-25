use crate::SyncError;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::Path,
};

pub const MAX_SUBTITLE_BYTES: u64 = 32 * 1024 * 1024;

/// Open a bounded regular input while allowing user-selected links to regular files.
#[doc(hidden)]
pub fn open_bounded_regular_file(path: &Path, limit: u64) -> io::Result<File> {
    let validate = |meta: &fs::Metadata| {
        if !meta.is_file() {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Sync input must be a regular file",
            ))
        } else if meta.len() > limit {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Sync input exceeds its size limit",
            ))
        } else {
            Ok(())
        }
    };
    validate(&fs::metadata(path)?)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // A FIFO substituted after metadata inspection must not block the open.
        options.custom_flags(nix::libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    validate(&file.metadata()?)?;
    Ok(file)
}

pub(crate) fn read_bytes(path: &Path, limit: u64, operation: &str) -> Result<Vec<u8>, SyncError> {
    crate::check_cancelled()?;
    let file =
        open_bounded_regular_file(path, limit).map_err(|error| SyncError::io(operation, error))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| SyncError::io(operation, error))?;
    if bytes.len() as u64 > limit {
        return Err(SyncError::io(
            operation,
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Sync input exceeds its size limit",
            ),
        ));
    }
    crate::check_cancelled()?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_file_limit_is_inclusive_and_directories_are_refused() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("input");
        fs::write(&path, b"12345678").unwrap();
        assert_eq!(read_bytes(&path, 8, "Read fixture").unwrap(), b"12345678");
        assert!(
            read_bytes(&path, 7, "Read fixture")
                .unwrap_err()
                .to_string()
                .contains("size limit")
        );
        assert!(
            open_bounded_regular_file(directory.path(), 8)
                .unwrap_err()
                .to_string()
                .contains("regular file")
        );
    }
}
