use crate::SyncError;
use std::{
    fs, io,
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

pub(crate) struct StagedFile {
    destination: PathBuf,
    file: NamedTempFile,
}

impl StagedFile {
    pub(crate) fn publish(self) -> Result<(), SyncError> {
        crate::check_cancelled()?;
        self.file
            .persist_noclobber(&self.destination)
            .map(|_| ())
            .map_err(|error| {
                SyncError::io(
                    "Publish complete output without replacing an existing file",
                    error.error,
                )
            })
    }
}

pub(crate) fn stage_file(
    destination: &Path,
    write: impl FnOnce(&mut fs::File) -> io::Result<()>,
) -> Result<StagedFile, SyncError> {
    crate::check_cancelled()?;
    match fs::symlink_metadata(destination) {
        Ok(_) => {
            return Err(SyncError::io(
                "Choose a new output path",
                io::ErrorKind::AlreadyExists.into(),
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(SyncError::io("Check output path", error)),
    }
    let directory = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    // Same-directory staging permits exclusive publication on the destination filesystem.
    let mut file = tempfile::Builder::new()
        .prefix(".subsync-")
        .suffix(".tmp")
        .tempfile_in(directory)
        .map_err(|error| SyncError::io("Create output staging file", error))?;
    write(file.as_file_mut())
        .and_then(|()| file.as_file_mut().flush())
        .and_then(|()| file.as_file().sync_all())
        .map_err(|error| SyncError::io("Write and synchronize complete staged output", error))?;
    Ok(StagedFile {
        destination: destination.to_owned(),
        file,
    })
}

pub(crate) fn stage_bytes(path: &Path, bytes: &[u8]) -> Result<StagedFile, SyncError> {
    stage_file(path, |file| file.write_all(bytes))
}

pub(crate) fn publish_pair(
    subtitle: StagedFile,
    receipt: Option<StagedFile>,
) -> Result<(), SyncError> {
    let subtitle_path = subtitle.destination.clone();
    subtitle.publish()?;
    if let Some(receipt) = receipt {
        receipt
            .publish()
            .map_err(|error| SyncError::ReportPublication {
                subtitle_path,
                source: Box::new(error),
            })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_failure_keeps_final_name_retryable() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("result.srt");
        let error = stage_file(&output, |file| {
            file.write_all(b"partial")?;
            Err(io::Error::other("authored disk write failure"))
        });
        assert!(error.is_err());
        assert!(!output.exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
        stage_bytes(&output, b"complete")
            .unwrap()
            .publish()
            .unwrap();
        assert_eq!(fs::read(output).unwrap(), b"complete");
    }

    #[test]
    fn a_race_winner_is_never_replaced_or_deleted() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("result.srt");
        let staged = stage_bytes(&output, b"our complete output").unwrap();
        fs::write(&output, b"independent winner").unwrap();
        assert!(staged.publish().is_err());
        assert_eq!(fs::read(&output).unwrap(), b"independent winner");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn late_report_collision_reports_complete_subtitle_and_preserves_winner() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("result.srt");
        let report = directory.path().join("report.json");
        let subtitle = stage_bytes(&output, b"complete subtitle").unwrap();
        let receipt = stage_bytes(&report, b"complete report").unwrap();
        fs::write(&report, b"independent winner").unwrap();
        let error = publish_pair(subtitle, Some(receipt)).unwrap_err();
        assert!(matches!(error, SyncError::ReportPublication { .. }));
        assert!(error.to_string().contains("Subtitle output is complete"));
        assert_eq!(fs::read(&output).unwrap(), b"complete subtitle");
        assert_eq!(fs::read(&report).unwrap(), b"independent winner");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn late_subtitle_collision_does_not_publish_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("result.srt");
        let report = directory.path().join("report.json");
        let subtitle = stage_bytes(&output, b"complete subtitle").unwrap();
        let receipt = stage_bytes(&report, b"complete report").unwrap();
        fs::write(&output, b"independent winner").unwrap();
        assert!(publish_pair(subtitle, Some(receipt)).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"independent winner");
        assert!(!report.exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
