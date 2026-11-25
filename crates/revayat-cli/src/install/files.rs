use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

pub const MAX_FILE: u64 = 16 * 1024 * 1024;
pub(super) const MAX_BYTES: u64 = 256 * 1024 * 1024;
pub(super) const MAX_ENTRIES: usize = 10_000;
pub type Identity = [String; 2];

pub fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub fn metadata(path: &Path) -> io::Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(m) => Ok(Some(m)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn linked(m: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    m.file_type().is_symlink()
}

pub fn safe_path(path: &Path, directory: bool) -> io::Result<PathBuf> {
    if path.components().any(|c| c == Component::ParentDir) {
        return Err(invalid(
            "Installation state path contains a parent traversal",
        ));
    }
    let path = std::path::absolute(path)?;
    let path: PathBuf = path.components().collect();
    if path.to_str().is_none_or(|s| s.contains('\0')) {
        return Err(invalid("Installation state path is not valid UTF-8"));
    }
    #[cfg(windows)]
    for component in path.components() {
        use std::path::Prefix;
        match component {
            Component::Prefix(prefix)
                if matches!(prefix.kind(), Prefix::DeviceNS(_) | Prefix::Verbatim(_)) =>
            {
                return Err(invalid(
                    "Installation state paths cannot use a device namespace",
                ));
            }
            Component::Normal(part)
                if part
                    .to_str()
                    .is_none_or(|s| s.contains(':') || s.ends_with(['.', ' '])) =>
            {
                return Err(invalid("Invalid installation state path component"));
            }
            _ => (),
        }
    }
    for entry in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
        if let Some(m) = metadata(entry)? {
            if linked(&m) {
                return Err(invalid(
                    "Installation state paths must not contain links or junctions",
                ));
            }
            if if directory || entry != path {
                !m.is_dir()
            } else {
                !m.is_file()
            } {
                return Err(invalid(
                    "Installation state path has an unexpected file type",
                ));
            }
        }
    }
    Ok(path)
}

pub fn identity_numbers(path: &Path) -> io::Result<(u64, u64)> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = fs::symlink_metadata(path)?;
        Ok((m.dev(), m.ino()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let f = OpenOptions::new()
            .access_mode(0)
            .custom_flags(0x0220_0000)
            .open(path)?;
        let m = winapi_util::file::information(&f)?;
        Ok((m.volume_serial_number(), m.file_index()))
    }
}

pub fn identity(path: &Path) -> io::Result<Identity> {
    identity_numbers(path).map(format_identity)
}

pub(super) fn format_identity((device, inode): (u64, u64)) -> Identity {
    [format!("{device:x}"), format!("{inode:x}")]
}

pub fn is_hex(s: &str, length: usize) -> bool {
    s.len() == length
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn valid_identity(value: &Value) -> bool {
    value.as_array().is_some_and(|a| {
        a.len() == 2
            && a.iter().all(|v| {
                v.as_str().is_some_and(|s| {
                    s == "0"
                        || !s.is_empty()
                            && s.len() <= 32
                            && !s.starts_with('0')
                            && is_hex(s, s.len())
                })
            })
    })
}

pub fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn read_limited(path: &Path, max: u64) -> io::Result<Vec<u8>> {
    let safe = safe_path(path, false)?;
    let mut options = OpenOptions::new();
    options.read(true);
    refuse_follow(&mut options);
    let file = options.open(&safe)?;
    let opened = file.metadata()?;
    if linked(&opened)
        || !opened.is_file()
        || !same_file::Handle::from_file(file.try_clone()?)?
            .eq(&same_file::Handle::from_path(safe_path(&safe, false)?)?)
    {
        return Err(invalid("Installation state file identity changed"));
    }
    safe_path(&safe, false)?;
    let mut bytes = Vec::new();
    file.take(max.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(invalid("Installation state file exceeds its byte limit"));
    }
    Ok(bytes)
}

fn refuse_follow(options: &mut OpenOptions) {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(OPEN_REPARSE_POINT);
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = options;
}

pub fn read_json(path: &Path) -> io::Result<Value> {
    Ok(serde_json::from_slice(&read_limited(path, MAX_FILE)?)?)
}

pub fn json_bytes(value: &Value) -> io::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAX_FILE {
        return Err(invalid("Installation journal exceeds its byte limit"));
    }
    Ok(bytes)
}

pub fn write_bytes(path: &Path, bytes: &[u8], replace: bool) -> io::Result<()> {
    let path = safe_path(path, false)?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid("Missing installation state parent"))?;
    let parent_id = identity(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    safe_path(&path, false)?;
    if identity(parent)? != parent_id {
        return Err(invalid("Installation state parent changed"));
    }
    if replace {
        file.persist(&path)?;
    } else {
        file.persist_noclobber(&path)?;
    }
    sync_directory(parent)
}

pub fn write_json(path: &Path, value: &Value) -> io::Result<()> {
    write_bytes(path, &json_bytes(value)?, true)
}

pub fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

pub fn move_owned(source: &Path, destination: &Path) -> io::Result<()> {
    let source = safe_path(source, true)?;
    let destination = safe_path(destination, true)?;
    if metadata(&destination)?.is_some() {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    #[cfg(target_os = "linux")]
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        &source,
        rustix::fs::CWD,
        &destination,
        rustix::fs::RenameFlags::NOREPLACE,
    )?;
    #[cfg(windows)]
    {
        let a = source
            .to_str()
            .filter(|p| !p.contains('\0'))
            .ok_or_else(|| invalid("Invalid source path"))?;
        let b = destination
            .to_str()
            .filter(|p| !p.contains('\0'))
            .ok_or_else(|| invalid("Invalid destination path"))?;
        winsafe::MoveFileEx(a, Some(b), winsafe::co::MOVEFILE::WRITE_THROUGH)
            .map_err(|e| io::Error::from_raw_os_error(e.raw() as i32))?;
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    return Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Exclusive directory rename unavailable",
    ));
    if let Some(parent) = source.parent() {
        sync_directory(parent)?;
    }
    if source.parent() != destination.parent()
        && let Some(parent) = destination.parent()
    {
        sync_directory(parent)?;
    }
    Ok(())
}

pub fn lock_owner(home: &Path) -> io::Result<(PathBuf, File)> {
    let home = safe_path(home, true)?;
    let root = safe_path(&home.join("revayat-install-state"), true)?;
    match fs::create_dir(&root) {
        Ok(()) => sync_directory(&home)?,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e),
    }
    let path = safe_path(&root.join("owner.lock"), false)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    refuse_follow(&mut options);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&path)?;
    let meta = file.metadata()?;
    if linked(&meta) || !meta.is_file() {
        return Err(invalid("Installation lock has an unexpected file type"));
    }
    let opened = same_file::Handle::from_file(file.try_clone()?)?;
    // Keep the existing fail-fast contention contract on the standard-library OS lock.
    file.try_lock().map_err(|e| match e {
        std::fs::TryLockError::WouldBlock => {
            invalid("Another same-account installation or recovery holds the OS lock")
        }
        std::fs::TryLockError::Error(e) => e,
    })?;
    if !file.metadata()?.is_file()
        || same_file::Handle::from_path(safe_path(&path, false)?)? != opened
    {
        return Err(invalid("Installation lock identity changed"));
    }
    Ok((root, file))
}

pub fn manifest_name(name: &str) -> bool {
    !name.contains(['\\', ':', '\0']) && name.split('/').all(|p| !matches!(p, "" | "." | ".."))
}

pub fn validate_manifest(v: &Value) -> io::Result<()> {
    if !v.as_object().is_some_and(|m| {
        m.contains_key("files")
            && m.contains_key("directories")
            && (m.len() == 2 || m.len() == 3 && m.contains_key("executables"))
    }) {
        return Err(invalid("Invalid installation tree manifest"));
    }
    let limit = || invalid("Installation tree manifest exceeds its entry limit");
    let files = v["files"].as_object().ok_or_else(limit)?;
    let directories = v["directories"].as_array().ok_or_else(limit)?;
    if files.len() + directories.len() > MAX_ENTRIES {
        return Err(limit());
    }
    if let Some(modes) = v.get("executables") {
        let invalid_modes = || invalid("Invalid installation execute-bit manifest");
        let modes = modes.as_object().ok_or_else(invalid_modes)?;
        if modes.len() > files.len()
            || modes.iter().any(|(name, mode)| {
                !files.contains_key(name)
                    || !mode.as_u64().is_some_and(|n| n > 0 && n & !0o111 == 0)
            })
        {
            return Err(invalid_modes());
        }
    }
    let mut names = std::collections::HashSet::with_capacity(files.len() + directories.len());
    for n in files
        .keys()
        .map(|n| Some(n.as_str()))
        .chain(directories.iter().map(Value::as_str))
    {
        let n = n
            .filter(|n| manifest_name(n))
            .ok_or_else(|| invalid("Invalid installation manifest relative path"))?;
        if !names.insert(n) {
            return Err(invalid("Duplicate installation manifest paths"));
        }
    }
    let mut total = 0;
    for v in files.values() {
        let fingerprint = || invalid("Invalid installation manifest fingerprint");
        let a = v
            .as_array()
            .filter(|a| a.len() == 2)
            .ok_or_else(fingerprint)?;
        total += a[0]
            .as_u64()
            .filter(|&n| n <= MAX_FILE)
            .ok_or_else(fingerprint)?;
        if !a[1].as_str().is_some_and(|s| is_hex(s, 64)) {
            return Err(fingerprint());
        }
    }
    if total > MAX_BYTES {
        return Err(invalid("Installation tree manifest exceeds its byte limit"));
    }
    Ok(())
}

pub fn fingerprint(path: &Path) -> io::Result<Value> {
    let bytes = read_limited(path, MAX_FILE)?;
    Ok(json!([bytes.len(), digest(&bytes)]))
}

pub fn tree_manifest(root: &Path) -> io::Result<Value> {
    inspect_tree(root).map(|(_, manifest)| manifest)
}

pub(super) fn inspect_tree(root: &Path) -> io::Result<(Identity, Value)> {
    let root = safe_path(root, true)?;
    let initial = identity(&root)?;
    let mut files = std::collections::BTreeMap::new();
    #[cfg(unix)]
    let mut executables = std::collections::BTreeMap::new();
    let mut directories = Vec::new();
    let mut total = 0;
    for (i, entry) in walkdir::WalkDir::new(&root)
        .min_depth(1)
        .follow_links(false)
        .into_iter()
        .enumerate()
    {
        if i >= MAX_ENTRIES {
            return Err(invalid(
                "Previous installation exceeds its complete-tree entry limit",
            ));
        }
        let entry = entry?;
        let path = entry.path();
        let m = fs::symlink_metadata(path)?;
        if linked(&m) {
            return Err(invalid(
                "Previous installation contains linked state; no partial backup was made",
            ));
        }
        let name = path
            .strip_prefix(&root)
            .map_err(io::Error::other)?
            .to_str()
            .ok_or_else(|| invalid("Installation path is not UTF-8"))?
            .replace(std::path::MAIN_SEPARATOR, "/");
        if m.is_dir() {
            directories.push(name);
        } else if m.is_file() {
            if m.len() > MAX_FILE || total + m.len() > MAX_BYTES {
                return Err(invalid(
                    "Previous installation exceeds its complete-tree byte limit",
                ));
            }
            let fp = fingerprint(path)?;
            total += fp[0]
                .as_u64()
                .ok_or_else(|| invalid("Invalid fingerprint"))?;
            if total > MAX_BYTES {
                return Err(invalid(
                    "Previous installation exceeds its complete-tree byte limit",
                ));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = m.permissions().mode() & 0o111;
                if mode != 0 {
                    executables.insert(name.clone(), mode);
                }
            }
            files.insert(name, fp);
        } else {
            return Err(invalid(
                "Previous installation contains a special file; no partial backup was made",
            ));
        }
    }
    if identity(&root)? != initial {
        return Err(invalid("Installation tree changed during inspection"));
    }
    directories.sort();
    let result = json!({"files":files,"directories":directories});
    #[cfg(unix)]
    let result = {
        let mut result = result;
        result["executables"] = json!(executables);
        result
    };
    validate_manifest(&result)?;
    Ok((initial, result))
}

pub fn matches(path: &Path, owned: &Value, manifest: &Value) -> bool {
    valid_identity(owned)
        && inspect_tree(path).is_ok_and(|(identity, current)| {
            json!(identity) == *owned && manifest_matches(&current, manifest)
        })
}

pub(super) fn manifest_matches(current: &Value, expected: &Value) -> bool {
    // Legacy authenticated journals did not capture execute bits of arbitrary old trees.
    current["files"] == expected["files"]
        && current["directories"] == expected["directories"]
        && expected
            .get("executables")
            .is_none_or(|modes| current.get("executables") == Some(modes))
}
