use same_file::Handle;
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
};

#[cfg(windows)]
pub fn rename_noreplace(source: &Path, destination: &Path) -> io::Result<()> {
    let source = std::path::absolute(source)?;
    let destination = std::path::absolute(destination)?;
    let text = |path: &Path| match path.to_str() {
        Some(s) if !s.contains('\0') => Ok(s.to_owned()),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Rename requires valid Windows path text",
        )),
    };
    winsafe::MoveFileEx(
        &text(&source)?,
        Some(&text(&destination)?),
        winsafe::co::MOVEFILE::WRITE_THROUGH,
    )
    .map_err(|code| io::Error::from_raw_os_error(code.raw() as i32))
}

#[cfg(target_os = "linux")]
pub fn rename_noreplace(source: &Path, destination: &Path) -> io::Result<()> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        source,
        rustix::fs::CWD,
        destination,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(Into::into)
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn rename_noreplace(_source: &Path, _destination: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "This platform lacks exclusive directory rename",
    ))
}

#[cfg(unix)]
fn sync_directory(path: &Path, opened: Option<&fs::File>) -> io::Result<()> {
    match opened {
        Some(file) => file.sync_all(),
        None => fs::File::open(path)?.sync_all(),
    }
}

#[cfg(not(unix))]
fn sync_directory(_: &Path, _: Option<&fs::File>) -> io::Result<()> {
    Ok(())
}

pub(crate) fn create_dir_all_synced(path: &Path) -> io::Result<()> {
    create_dir_all_synced_with(path, |parent| sync_directory(parent, None))
}

fn create_dir_all_synced_with(
    path: &Path,
    mut sync: impl FnMut(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let path = std::path::absolute(path)?;
    let mut parents = Vec::new();
    for directory in path.ancestors() {
        if fs::metadata(directory).is_ok_and(|metadata| metadata.is_dir()) {
            break;
        }
        if let Some(parent) = directory.parent() {
            parents.push(parent);
        }
    }
    fs::create_dir_all(&path)?;
    for parent in parents.into_iter().rev() {
        sync(parent)?;
    }
    Ok(())
}

pub fn publish_bytes(destination: &Path, data: &[u8]) -> io::Result<()> {
    publish_bytes_with_sync(destination, data, create_dir_all_synced, sync_directory)
}

fn publish_bytes_with_sync(
    destination: &Path,
    data: &[u8],
    create_dirs: impl FnOnce(&Path) -> io::Result<()>,
    sync: impl FnOnce(&Path, Option<&fs::File>) -> io::Result<()>,
) -> io::Result<()> {
    let destination = std::path::absolute(destination)?;
    match fs::symlink_metadata(&destination) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Output already exists; use a new filename",
            ));
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(e),
    }
    let parent = destination.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Output requires a parent directory",
        )
    })?;
    create_dirs(parent)?;
    #[cfg(unix)]
    let parent_handle = Some(fs::File::open(parent)?);
    #[cfg(not(unix))]
    let parent_handle = None;
    let (mut file, temporary) = tempfile::Builder::new()
        .prefix(".publish-")
        .suffix(".tmp")
        .tempfile_in(parent)?
        .into_parts();
    let owned = Handle::from_file(file.try_clone()?)?;
    let stage = temporary.keep().map_err(|e| e.error)?;
    let result = (|| {
        file.write_all(data)?;
        file.sync_all()?;
        let mut verified = Vec::new();
        let current = Handle::from_path(&stage)?;
        if current != owned || fs::symlink_metadata(&stage)?.file_type().is_symlink() {
            return Err(io::Error::other("Staged publication identity changed"));
        }
        current
            .as_file()
            .take(data.len().saturating_add(1) as u64)
            .read_to_end(&mut verified)?;
        if verified != data {
            return Err(io::Error::other("Staged publication bytes changed"));
        }
        rename_noreplace(&stage, &destination)?;
        sync(parent, parent_handle.as_ref()).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("Publication committed; directory synchronization failed: {e}"),
            )
        })?;
        Ok(())
    })();
    if fs::symlink_metadata(&stage).is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
        && Handle::from_path(&stage).is_ok_and(|h| h == owned)
        && let Err(e) = fs::remove_file(&stage)
        && e.kind() != io::ErrorKind::NotFound
    {
        crate::runtime::logging::stderr_line("WARNING: Publication staging cleanup failed.");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syncs_each_new_directory_entry_and_the_published_file_entry() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        let base = std::path::absolute(root.path())?;
        let output = base.join("a/b/c/out.zip");
        let synced = std::cell::RefCell::new(Vec::new());
        let sync = |path: &Path| {
            synced.borrow_mut().push(path.to_owned());
            Ok(())
        };
        publish_bytes_with_sync(
            &output,
            b"published",
            |parent| create_dir_all_synced_with(parent, sync),
            |parent, _| sync(parent),
        )?;
        assert_eq!(
            synced.into_inner(),
            [
                base.clone(),
                base.join("a"),
                base.join("a/b"),
                base.join("a/b/c")
            ]
        );
        assert_eq!(fs::read(output)?, b"published");
        Ok(())
    }

    #[test]
    fn a_post_commit_sync_failure_reports_that_the_file_was_published() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        let output = root.path().join("out.zip");
        let result = publish_bytes_with_sync(
            &output,
            b"committed bytes",
            create_dir_all_synced,
            |_, _| Err(io::Error::other("injected directory sync failure")),
        );
        assert!(result.is_err_and(|error| {
            error
                .to_string()
                .contains("Publication committed; directory synchronization failed")
        }));
        assert_eq!(fs::read(output)?, b"committed bytes");
        Ok(())
    }

    #[test]
    fn publishes_once_and_keeps_collision_winner() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        let output = root.path().join("space here/زیرنویس.ass");
        publish_bytes(&output, b"original")?;
        assert!(
            matches!(publish_bytes(&output,b"replacement"),Err(e) if e.kind()==io::ErrorKind::AlreadyExists)
        );
        assert_eq!(fs::read(&output)?, b"original");
        Ok(())
    }
    #[test]
    fn refuses_to_replace_existing_directory() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        let from = root.path().join("from");
        let to = root.path().join("to");
        fs::create_dir(&from)?;
        fs::create_dir(&to)?;
        assert!(rename_noreplace(&from, &to).is_err());
        assert!(from.is_dir() && to.is_dir());
        fs::remove_dir(&to)?;
        rename_noreplace(&from, &to)?;
        assert!(!from.exists() && to.is_dir());
        Ok(())
    }
    #[test]
    fn dangling_symlink_counts_as_existing_output() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        let target = root.path().join("missing");
        let link = root.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link)?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&target, &link)?;
        assert!(
            matches!(publish_bytes(&link,b"new"),Err(e) if e.kind()==io::ErrorKind::AlreadyExists)
        );
        assert_eq!(fs::read_link(&link)?, target);
        assert!(!target.exists());
        Ok(())
    }
}
