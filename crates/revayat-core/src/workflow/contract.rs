use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub fn python_type(value: &Value) -> &str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_f64() => "float",
        Value::Number(_) => "int",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(m) if m.len() == 1 => match m.keys().next().map(String::as_str) {
            Some("$tuple") => "tuple",
            Some("$set") => "set",
            Some("$float" | "$num") => "float",
            Some("$int") => "int",
            Some("$bytes") => "bytes",
            _ => "dict",
        },
        Value::Object(_) if value.get("attrs").is_some() => {
            value["$object"].as_str().unwrap_or("dict")
        }
        Value::Object(_) => "dict",
    }
}

pub fn attribute_error(value: &Value, attr: &str) -> String {
    format!(
        "AttributeError: '{}' object has no attribute '{attr}'",
        python_type(value)
    )
}

pub fn attr_path(value: &Value, attr: &str) -> Result<PathBuf, String> {
    path(value).ok_or_else(|| attribute_error(value, attr))
}

pub fn workspace_path(value: &Value) -> Result<PathBuf, String> {
    path(value).ok_or_else(|| {
        format!(
            "TypeError: unsupported operand type(s) for /: '{}' and 'str'",
            python_type(value)
        )
    })
}

pub fn path_marker(path: &Path) -> Result<Value, String> {
    Ok(json!({"$path": path.to_str().ok_or_else(|| value_error("Path must use valid UTF-8"))?}))
}

fn path(value: &Value) -> Option<PathBuf> {
    (value.as_object()?.len() == 1).then_some(())?;
    Some(PathBuf::from(value.get("$path")?.as_str()?))
}

pub fn item<'a>(value: &'a Value, key: &str) -> Result<&'a Value, String> {
    match python_type(value) {
        "dict" => value.get(key).ok_or_else(|| format!("KeyError: '{key}'")),
        "list" | "tuple" => Err(format!(
            "TypeError: {} indices must be integers or slices, not str",
            python_type(value)
        )),
        "str" => Err("TypeError: string indices must be integers, not 'str'".into()),
        kind => Err(format!("TypeError: '{kind}' object is not subscriptable")),
    }
}

pub fn items(value: &Value) -> &[Value] {
    value.as_array().map_or(&[], Vec::as_slice)
}

pub fn iterable(value: &Value) -> Result<&[Value], String> {
    value
        .as_array()
        .or_else(|| value.get("$tuple").and_then(Value::as_array))
        .or_else(|| value.get("$set").and_then(Value::as_array))
        .map(Vec::as_slice)
        .ok_or_else(|| format!("TypeError: '{}' object is not iterable", python_type(value)))
}

pub fn string(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

pub fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(m) if m.len() == 1 => {
            m.iter().next().is_some_and(|(k, v)| match k.as_str() {
                "$tuple" | "$set" | "$bytes" | "$dict" => truth(v),
                "$num" => v != "-0",
                "$undefined" => false,
                _ => true,
            })
        }
        Value::Object(m) => !m.is_empty(),
    }
}

pub fn value_error(message: impl std::fmt::Display) -> String {
    format!("ValueError: {message}")
}

pub fn io_error(error: std::io::Error) -> String {
    let kind = match error.kind() {
        std::io::ErrorKind::NotFound => "FileNotFoundError",
        std::io::ErrorKind::AlreadyExists => "FileExistsError",
        std::io::ErrorKind::PermissionDenied => "PermissionError",
        std::io::ErrorKind::NotADirectory => "NotADirectoryError",
        std::io::ErrorKind::IsADirectory => "IsADirectoryError",
        _ => "OSError",
    };
    format!("{kind}: {error}")
}

pub use crate::runtime::files::linked;

pub fn marked<T: serde::Serialize>(name: &str, value: &T) -> Result<Value, String> {
    let mut attrs = serde_json::to_value(value).map_err(value_error)?;
    if name == "Document" {
        if let Some(cues) = attrs.get_mut("cues").and_then(Value::as_array_mut) {
            for cue in cues {
                *cue = json!({"$object":"Cue", "attrs": std::mem::take(cue)});
            }
        }
        if let Some(sections) = attrs.get_mut("sections").and_then(Value::as_array_mut) {
            for section in sections {
                *section = json!({"$tuple": std::mem::take(section)});
            }
        }
    }
    Ok(json!({"$object": name, "attrs": attrs}))
}

pub fn decode<T: serde::de::DeserializeOwned>(
    name: &str,
    value: Value,
    first_attr: &str,
) -> Result<T, String> {
    if value["$object"] != name {
        return Err(attribute_error(&value, first_attr));
    }
    let mut attrs = value["attrs"].clone();
    if name == "Document" {
        if let Some(cues) = attrs.get_mut("cues").and_then(Value::as_array_mut) {
            for cue in cues {
                if cue["$object"] == "Cue" {
                    *cue = std::mem::take(&mut cue["attrs"]);
                }
            }
        }
        if let Some(sections) = attrs.get_mut("sections").and_then(Value::as_array_mut) {
            for section in sections {
                if let Some(tuple) = section.get_mut("$tuple") {
                    *section = std::mem::take(tuple);
                }
            }
        }
    }
    serde_json::from_value(attrs).map_err(|err| format!("TypeError: invalid {name}: {err}"))
}
