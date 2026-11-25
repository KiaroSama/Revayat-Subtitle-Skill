use super::contract::value_error;
use serde_json::Value;
use std::fmt::Write;

// Edition IDs hash Python's JSON bytes, including spaces and exponent spelling.
pub fn dump(value: &Value, sorted: bool, ascii: bool) -> Result<String, String> {
    dump_mode(value, sorted, ascii, true)
}

/// Serialize persisted JSON literally, without interpreting compatibility markers.
pub fn dump_json(value: &Value, sorted: bool, ascii: bool) -> Result<String, String> {
    dump_mode(value, sorted, ascii, false)
}

fn compare_keys(left: &Value, right: &Value) -> Result<std::cmp::Ordering, String> {
    use std::cmp::Ordering;
    let integer = |value: &Value| {
        value
            .as_bool()
            .map(i128::from)
            .or_else(|| value.as_i64().map(i128::from))
            .or_else(|| value.as_u64().map(i128::from))
    };
    let int_float = |integer: i128, float: f64| {
        integer.cmp(&(float as i128)).then_with(|| {
            if float.fract() == 0.0 {
                Ordering::Equal
            } else if float.is_sign_negative() {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        })
    };
    match (left, right) {
        (Value::String(a), Value::String(b)) => Ok(a.cmp(b)),
        (Value::Null, Value::Null) => Ok(Ordering::Equal),
        _ => match (integer(left), integer(right), left.as_f64(), right.as_f64()) {
            (Some(a), Some(b), _, _) => Ok(a.cmp(&b)),
            (Some(a), None, _, Some(b)) => Ok(int_float(a, b)),
            (None, Some(b), Some(a), _) => Ok(int_float(b, a).reverse()),
            (None, None, Some(a), Some(b)) => a
                .partial_cmp(&b)
                .ok_or_else(|| value_error("Nonfinite dictionary key")),
            _ => Err(format!(
                "TypeError: '<' not supported between instances of '{}' and '{}'",
                super::contract::python_type(left),
                super::contract::python_type(right)
            )),
        },
    }
}

fn dump_mode(value: &Value, sorted: bool, ascii: bool, bridge: bool) -> Result<String, String> {
    fn text(out: &mut String, value: &str, ascii: bool) -> Result<(), String> {
        let json = serde_json::to_string(value).map_err(value_error)?;
        for ch in json.chars() {
            if ascii && !matches!(ch, '\x20'..='\x7e') {
                for unit in ch.encode_utf16(&mut [0; 2]) {
                    write!(out, "\\u{unit:04x}").map_err(value_error)?;
                }
            } else {
                out.push(ch);
            }
        }
        Ok(())
    }
    fn float(out: &mut String, f: f64) -> Result<(), String> {
        if !f.is_finite() {
            return Err(value_error(
                "Out of range float values are not JSON compliant",
            ));
        }
        if f != 0.0 && !(1e-4..1e16).contains(&f.abs()) {
            let raw = format!("{f:e}");
            let (coefficient, exponent) = raw
                .split_once('e')
                .ok_or_else(|| value_error("Invalid float exponent"))?;
            let exponent: i32 = exponent.parse().map_err(value_error)?;
            write!(out, "{coefficient}e{exponent:+03}").map_err(value_error)?;
        } else {
            write!(out, "{f:?}").map_err(value_error)?;
        }
        Ok(())
    }
    fn add(
        out: &mut String,
        value: &Value,
        sorted: bool,
        ascii: bool,
        bridge: bool,
    ) -> Result<(), String> {
        match value {
            Value::String(s) => text(out, s, ascii)?,
            Value::Number(n) if n.is_f64() => float(
                out,
                n.as_f64().ok_or_else(|| value_error("Invalid number"))?,
            )?,
            Value::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    add(out, item, sorted, ascii, bridge)?;
                }
                out.push(']');
            }
            Value::Object(map) if bridge && map.len() == 1 && map.contains_key("$num") => {
                if map["$num"] != "-0" {
                    return Err(value_error(
                        "Out of range float values are not JSON compliant",
                    ));
                }
                out.push_str("-0.0");
            }
            Value::Object(map)
                if bridge
                    && map.len() == 1
                    && map.get("$int").and_then(Value::as_str).is_some() =>
            {
                out.push_str(map["$int"].as_str().unwrap_or_default());
            }
            Value::Object(map)
                if bridge
                    && map.len() == 1
                    && map.get("$float").and_then(Value::as_str).is_some() =>
            {
                float(
                    out,
                    map["$float"]
                        .as_str()
                        .unwrap_or_default()
                        .parse()
                        .map_err(value_error)?,
                )?;
            }
            Value::Object(map)
                if bridge && map.len() == 1 && map.get("$tuple").is_some_and(Value::is_array) =>
            {
                add(out, &map["$tuple"], sorted, ascii, bridge)?
            }
            Value::Object(map)
                if bridge && map.len() == 1 && map.get("$dict").is_some_and(Value::is_array) =>
            {
                let mut pairs: Vec<_> = super::contract::items(&map["$dict"]).iter().collect();
                if pairs
                    .iter()
                    .any(|pair| pair.as_array().is_none_or(|items| items.len() != 2))
                {
                    return Err(value_error("Dictionary marker requires key/value pairs"));
                }
                if sorted && let Some(first) = pairs.first() {
                    for pair in &pairs {
                        compare_keys(&first[0], &pair[0])?;
                    }
                    pairs.sort_by(|a, b| {
                        compare_keys(&a[0], &b[0]).expect("validated comparable keys")
                    });
                }
                out.push('{');
                for (i, pair) in pairs.into_iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    let key = &pair[0];
                    if key.is_object() || key.is_array() {
                        return Err(format!(
                            "TypeError: keys must be str, int, float, bool or None, not {}",
                            super::contract::python_type(key)
                        ));
                    }
                    let mut raw = String::new();
                    if let Some(key) = key.as_str() {
                        raw.push_str(key);
                    } else {
                        add(&mut raw, key, sorted, ascii, bridge)?;
                    }
                    text(out, &raw, ascii)?;
                    out.push_str(": ");
                    add(out, &pair[1], sorted, ascii, bridge)?;
                }
                out.push('}');
            }
            Value::Object(map)
                if bridge
                    && map.len() == 1
                    && (map.contains_key("$bytes") || map.contains_key("$set")) =>
            {
                return Err(format!(
                    "TypeError: Object of type {} is not JSON serializable",
                    super::contract::python_type(value)
                ));
            }
            Value::Object(map) => {
                let mut entries: Vec<_> = map.iter().collect();
                if sorted {
                    entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
                }
                out.push('{');
                for (i, (key, item)) in entries.into_iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    text(out, key, ascii)?;
                    out.push_str(": ");
                    add(out, item, sorted, ascii, bridge)?;
                }
                out.push('}');
            }
            _ => write!(out, "{value}").map_err(value_error)?,
        }
        Ok(())
    }
    let mut result = String::new();
    add(&mut result, value, sorted, ascii, bridge)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_python_json_spacing_unicode_and_exponents() -> Result<(), String> {
        assert_eq!(
            dump(
                &json!({"z": 1e-5, "a": ["سلام", "🙂", 1e16, 2.0]}),
                true,
                true
            )?,
            r#"{"a": ["\u0633\u0644\u0627\u0645", "\ud83d\ude42", 1e+16, 2.0], "z": 1e-05}"#
        );
        Ok(())
    }
}
