use same_file::Handle;
use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

pub const MAX_JSON_BYTES: u64 = 64 * 1024 * 1024;
fn os(e: io::Error) -> String {
    let kind = match e.kind() {
        io::ErrorKind::AlreadyExists => "FileExistsError",
        io::ErrorKind::NotFound => "FileNotFoundError",
        io::ErrorKind::PermissionDenied => "PermissionError",
        _ => "OSError",
    };
    format!("{kind}: {e}")
}
fn basename(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
}
fn value(message: &str) -> String {
    format!("ValueError: {message}")
}

pub(crate) fn open_artifact(path: &Path) -> io::Result<File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use rustix::fs::OFlags;
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags((OFlags::NONBLOCK | OFlags::NOFOLLOW).bits() as i32);
    }
    options.open(path)
}

pub fn read_limited(path: &Path, maximum: u64) -> Result<Vec<u8>, String> {
    read_limited_with_metadata(path, maximum).map(|(data, _)| data)
}

pub(crate) fn read_limited_with_metadata(
    path: &Path,
    maximum: u64,
) -> Result<(Vec<u8>, fs::Metadata), String> {
    let name = basename(path);
    if !fs::metadata(path).map_err(os)?.is_file() {
        return Err(value(&format!("{name}: expected a regular artifact file")));
    }
    let reader = open_artifact(path).map_err(os)?;
    let metadata = reader.metadata().map_err(os)?;
    if !metadata.is_file() {
        return Err(value(&format!(
            "{name}: artifact type changed while opening"
        )));
    }
    let mut data = Vec::new();
    reader
        .take(maximum.saturating_add(1))
        .read_to_end(&mut data)
        .map_err(os)?;
    if data.len() as u64 > maximum {
        return Err(value(&format!(
            "{name}: file exceeds the supported byte limit"
        )));
    }
    Ok((data, metadata))
}

fn unchanged(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    #[cfg(unix)]
    let change_time_matches = {
        use std::os::unix::fs::MetadataExt;
        (a.ctime(), a.ctime_nsec()) == (b.ctime(), b.ctime_nsec())
    };
    #[cfg(not(unix))]
    let change_time_matches = true;
    change_time_matches && a.len() == b.len() && a.modified().ok() == b.modified().ok()
}
pub fn file_fingerprint(path: &Path, maximum: u64) -> Result<Value, String> {
    let before = fs::metadata(path).map_err(os)?;
    if !before.is_file() {
        return Err(value("Fingerprint input must be a regular file"));
    }
    if before.len() > maximum {
        return Err(value(
            "Fingerprint input exceeds the supported file-size limit",
        ));
    }
    let mut reader = Handle::from_file(open_artifact(path).map_err(os)?).map_err(os)?;
    let opened = reader.as_file().metadata().map_err(os)?;
    if !opened.is_file() || !unchanged(&opened, &before) {
        return Err(value("Fingerprint input changed while opening"));
    }
    let mut sha = Sha256::new();
    let mut total = 0_u64;
    let mut buf = vec![0; 1024 * 1024];
    loop {
        let limit = usize::try_from((maximum - total).saturating_add(1).min(buf.len() as u64))
            .map_err(|_| value("Invalid file read size"))?;
        let n = reader.as_file_mut().read(&mut buf[..limit]).map_err(os)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > maximum {
            return Err(value("Fingerprint input grew beyond its size limit"));
        }
        sha.update(&buf[..n]);
    }
    let finished = reader.as_file().metadata().map_err(os)?;
    let after = Handle::from_file(open_artifact(path).map_err(os)?).map_err(os)?;
    if total != opened.len()
        || !unchanged(&opened, &finished)
        || reader != after
        || !unchanged(&before, &after.as_file().metadata().map_err(os)?)
    {
        return Err(value("Fingerprint input changed while reading"));
    }
    Ok(json!({"name":basename(path),"bytes":total,"sha256":hex::encode(sha.finalize())}))
}

struct Checked;
impl<'de> DeserializeSeed<'de> for Checked {
    type Value = Value;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Checked {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("valid JSON")
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        serde_json::Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("non-finite JSON number"))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
        let mut out = Vec::new();
        while let Some(v) = a.next_element_seed(Checked)? {
            out.push(v);
        }
        Ok(Value::Array(out))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
        let mut out = Map::new();
        while let Some(key) = a.next_key::<String>()? {
            if out.contains_key(&key) {
                return Err(de::Error::custom("duplicate object key"));
            }
            out.insert(key, a.next_value_seed(Checked)?);
        }
        Ok(Value::Object(out))
    }
}
fn number_budget(data: &[u8]) -> bool {
    let mut i = 0;
    let mut quoted = false;
    while let Some(&b) = data.get(i) {
        if quoted && b == b'\\' {
            i += 2;
            continue;
        }
        if b == b'"' {
            quoted = !quoted;
        }
        if !quoted && (b.is_ascii_digit() || b == b'-') {
            let start = i;
            let mut fraction = false;
            while let Some(b) = data
                .get(i)
                .filter(|b| b.is_ascii_digit() || b".eE+-".contains(b))
            {
                fraction |= matches!(b, b'.' | b'e' | b'E');
                i += 1;
            }
            if i - start > if fraction { 64 } else { 21 } {
                return false;
            }
            continue;
        }
        i += 1;
    }
    true
}
pub fn read_json(path: &Path, maximum: u64) -> Result<Value, String> {
    let maximum = maximum.min(MAX_JSON_BYTES);
    let name = basename(path);
    if fs::metadata(path).map_err(os)?.len() > maximum {
        return Err(value(&format!("{name}: JSON exceeds its byte limit")));
    }
    let invalid = |label: &str| value(&format!("{name}: invalid JSON ({label})"));
    let bytes = read_limited(path, maximum).map_err(|e| {
        if e.starts_with("ValueError:") {
            invalid("ValueError")
        } else {
            e
        }
    })?;
    parse_json(&bytes, &name)
}

pub(crate) fn parse_json(bytes: &[u8], name: &str) -> Result<Value, String> {
    let invalid = |label: &str| value(&format!("{name}: invalid JSON ({label})"));
    let source = std::str::from_utf8(bytes).map_err(|_| invalid("UnicodeDecodeError"))?;
    if !number_budget(bytes) {
        return Err(invalid("ValueError"));
    }
    let mut parser = serde_json::Deserializer::from_str(source);
    let decoded = Checked.deserialize(&mut parser).and_then(|v| {
        parser.end()?;
        Ok(v)
    });
    decoded.map_err(|e| {
        invalid(if e.to_string().starts_with("recursion limit exceeded") {
            "RecursionError"
        } else {
            "JSONDecodeError"
        })
    })
}

pub fn linked(m: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 1024 != 0 {
            return true;
        }
    }
    m.file_type().is_symlink()
}
pub fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    write_json_with_commit(path, value, || {})
}

pub(crate) fn write_json_with_commit(
    path: &Path,
    value: &Value,
    committed: impl FnOnce(),
) -> Result<(), String> {
    // serde_json's reader rejects the 128th nested object or array.
    fn within_depth(value: &Value, remaining: usize) -> bool {
        match value {
            Value::Array(items) => {
                remaining > 0 && items.iter().all(|item| within_depth(item, remaining - 1))
            }
            Value::Object(items) => {
                remaining > 0 && items.values().all(|item| within_depth(item, remaining - 1))
            }
            _ => true,
        }
    }
    if !within_depth(value, 127) {
        return Err(format!(
            "ValueError: {}: invalid JSON (RecursionError)",
            basename(path)
        ));
    }
    let mut data = serde_json::to_vec_pretty(value)
        .map_err(|_| String::from("ValueError: Value cannot be written as JSON"))?;
    data.push(b'\n');
    if data.len() as u64 > MAX_JSON_BYTES {
        return Err(format!(
            "ValueError: {}: JSON exceeds its byte limit",
            basename(path)
        ));
    }
    let absolute = std::path::absolute(path).map_err(os)?;
    let parent = absolute
        .parent()
        .ok_or_else(|| String::from("ValueError: JSON path requires a parent"))?;
    fs::create_dir_all(parent).map_err(os)?;
    let (mut file, temp) = tempfile::Builder::new()
        .prefix(".write-")
        .tempfile_in(parent)
        .map_err(os)?
        .into_parts();
    let owned = Handle::from_file(file.try_clone().map_err(os)?).map_err(os)?;
    let stage = temp.keep().map_err(|e| os(e.error))?;
    let still_owned = || {
        fs::symlink_metadata(&stage).is_ok_and(|m| m.is_file() && !linked(&m))
            && Handle::from_path(&stage).is_ok_and(|h| h == owned)
    };
    let result = (|| {
        file.write_all(&data)
            .and_then(|()| file.sync_all())
            .map_err(os)?;
        if !still_owned() || read_limited(&stage, data.len() as u64)? != data {
            return Err("OSError: Staged JSON bytes or identity changed".into());
        }
        fs::rename(&stage, &absolute).map_err(os)?;
        committed();
        if read_limited(&absolute, data.len() as u64)? != data {
            return Err(
                "OSError: Published JSON bytes changed; verify the saved state before retrying"
                    .into(),
            );
        }
        #[cfg(unix)]
        File::open(parent).and_then(|f| f.sync_all()).map_err(|_| {
            String::from("OSError: JSON committed; directory synchronization failed")
        })?;
        Ok(())
    })();
    if still_owned()
        && let Err(e) = fs::remove_file(&stage)
        && e.kind() != io::ErrorKind::NotFound
    {
        super::logging::stderr_line(
            "WARNING: JSON staging cleanup failed; original operation result is preserved",
        );
    }
    result
}

pub(super) fn check_relative(s: &str) -> Result<(), String> {
    if s.contains(['\\', ':', '\0']) || s.split('/').any(|p| matches!(p, "" | "." | "..")) {
        return Err(value("Working path must be a canonical relative path"));
    }
    Ok(())
}
pub fn local_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    check_relative(relative)?;
    let root = std::path::absolute(root).map_err(os)?;
    let mut path = root.clone();
    for part in std::iter::once(None).chain(relative.split('/').map(Some)) {
        if let Some(part) = part {
            path.push(part);
        }
        match fs::symlink_metadata(&path) {
            Ok(m) if linked(&m) => return Err(value("Linked working artifacts are not accepted")),
            Ok(_) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(os(e)),
        }
    }
    let canonical = |p: &Path| -> Result<PathBuf, String> {
        for ancestor in p.ancestors() {
            match fs::canonicalize(ancestor) {
                Ok(a) => {
                    return p
                        .strip_prefix(ancestor)
                        .map(|rest| {
                            if rest.as_os_str().is_empty() {
                                a.clone()
                            } else {
                                a.join(rest)
                            }
                        })
                        .map_err(|_| value("Working file escapes its workspace"));
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                Err(e) => return Err(os(e)),
            }
        }
        Err(value("Working file escapes its workspace"))
    };
    let base = canonical(&root)?;
    let resolved = canonical(&path)?;
    if !resolved.starts_with(base) {
        return Err(value("Working file escapes its workspace"));
    }
    Ok(resolved)
}
pub fn output_directory(parent: &Path, prefix: &str) -> Result<PathBuf, String> {
    if prefix.contains(['\\', '/', ':', '\0']) {
        return Err(value("Output prefix must not contain a path"));
    }
    let parent = fs::canonicalize(parent).map_err(os)?;
    Ok(tempfile::Builder::new()
        .prefix(prefix)
        .rand_bytes(32)
        .tempdir_in(parent)
        .map_err(os)?
        .keep())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_writes_and_fingerprints_utf8_json() -> Result<(), String> {
        let root = tempfile::tempdir().map_err(os)?;
        let p = local_path(root.path(), "nested dir/state.json")?;
        let record = json!({"text":"زیرنویس","ready":true});
        write_json(&p, &record)?;
        assert_eq!(read_json(&p, 512)?, record);
        let raw = read_limited(&p, 512)?;
        assert!(read_limited(&p, 1).is_err());
        assert_eq!(
            file_fingerprint(&p, 512)?["sha256"],
            hex::encode(Sha256::digest(raw))
        );
        assert!(local_path(root.path(), "../outside").is_err());
        Ok(())
    }
    #[test]
    fn rejects_duplicate_or_oversized_numbers_without_echoing_the_record() -> Result<(), String> {
        let root = tempfile::tempdir().map_err(os)?;
        let p = root.path().join("state.json");
        for raw in [
            b"{\"private\":1,\"private\":2}".as_slice(),
            b"1234567890123456789012".as_slice(),
        ] {
            fs::write(&p, raw).map_err(os)?;
            let result = read_json(&p, 512);
            assert!(
                matches!(result,Err(e) if e.contains("invalid JSON") && !e.contains("private"))
            );
        }
        Ok(())
    }
}
