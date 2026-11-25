//! Bounded artifact I/O, subprocesses, and diagnostics.
#![forbid(unsafe_code)]
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;
pub mod assets;
pub mod diagnostic;
pub mod logging;
pub use logging::{LogLevel, OperationalLog};
pub mod files;
pub use crate::process_control::{RunError, RunOptions, run, run_owned};
#[cfg(test)]
mod tests;

pub fn digest(data: Vec<u8>) -> Result<String, String> {
    Ok(digest_bytes(&data))
}

pub fn digest_bytes(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}
pub fn same_json(actual: Value, expected: Value) -> Result<bool, String> {
    Ok(equal(&actual, &expected))
}
fn equal(actual: &Value, expected: &Value) -> bool {
    let mut pending = vec![(actual, expected)];
    while let Some((a, b)) = pending.pop() {
        match (a, b) {
            (Value::Object(a), Value::Object(b)) if a.len() == b.len() => {
                if a.len() == 1 && a.get("$num").and_then(Value::as_str) == Some("NaN") {
                    return false;
                }
                for (key, value) in a {
                    let Some(other) = b.get(key) else {
                        return false;
                    };
                    pending.push((value, other));
                }
            }
            (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
                pending.extend(a.iter().zip(b))
            }
            (Value::Number(a), Value::Number(b)) if a.is_f64() != b.is_f64() => return false,
            (a, b) if a != b => return false,
            _ => (),
        }
    }
    true
}
fn python_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Number(n) if n.is_f64() => "float",
        Value::Number(_) => "int",
        Value::Object(_) => "dict",
    }
}
fn path<'a>(v: &'a Value, method: &str) -> Result<&'a Path, String> {
    v.get("$path")
        .and_then(Value::as_str)
        .map(Path::new)
        .ok_or_else(|| {
            format!(
                "AttributeError: '{}' object has no attribute '{method}'",
                python_type(v)
            )
        })
}
fn limit(value: i64, what: &str) -> Result<u64, String> {
    u64::try_from(value)
        .map_err(|_| format!("ValueError: {what} byte limit must be a nonnegative integer"))
}
pub fn read_limited(path_value: Value, maximum: i64) -> Result<Vec<u8>, String> {
    let maximum = limit(maximum, "File")?;
    files::read_limited(path(&path_value, "stat")?, maximum)
}
pub fn file_fingerprint(path_value: Value, maximum: Option<i64>) -> Result<Value, String> {
    let maximum = limit(maximum.unwrap_or(16 * 1024_i64.pow(3)), "Fingerprint")?;
    files::file_fingerprint(path(&path_value, "stat")?, maximum)
}
pub fn read_json(path_value: Value, maximum: Option<i64>) -> Result<Value, String> {
    let maximum = limit(maximum.unwrap_or(files::MAX_JSON_BYTES as i64), "JSON")?;
    files::read_json(path(&path_value, "stat")?, maximum)
}
pub fn write_json(path_value: Value, value: Value) -> Result<(), String> {
    files::write_json(path(&path_value, "parent")?, &value)
}
pub fn local_path(root: Value, relative: String) -> Result<Value, String> {
    files::check_relative(&relative)?;
    let root = root.get("$path").and_then(Value::as_str).ok_or_else(|| {
        format!(
            "TypeError: unsupported operand type(s) for /: '{}' and 'str'",
            python_type(&root)
        )
    })?;
    Ok(json!({"$path":files::local_path(Path::new(root),&relative)?}))
}
pub fn output_directory(parent: Value, prefix: String) -> Result<Value, String> {
    Ok(json!({"$path":files::output_directory(path(&parent,"resolve")?,&prefix)?}))
}
pub fn log_fallback(component: String, level: String, message: String) -> Result<Value, String> {
    logging::stderr_line(&diagnostic::line(&component, &level, &message));
    Ok(Value::Null)
}
