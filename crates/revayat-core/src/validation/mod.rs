//! Typed validation with content-free diagnostics.
#![forbid(unsafe_code)]
use serde_json::Value;
pub(crate) mod encoding;
mod records;
#[cfg(test)]
mod tests;
pub const MAX_TIME_MS: i64 = 1_000_000_000_000;

pub fn fail(r#where: String, expected: String) -> Result<Value, String> {
    Err(invalid(&r#where, &expected))
}
fn invalid(location: &str, detail: &str) -> String {
    format!("ValueError: {location}: {detail}")
}
fn object(value: &Value, loc: &str) -> Result<(), String> {
    if value.is_object()
        && !value.as_object().is_some_and(|m| {
            m.len() == 1
                && ["$bytes", "$tuple", "$set", "$int", "$num", "$float"]
                    .iter()
                    .any(|k| m.contains_key(*k))
        })
    {
        Ok(())
    } else {
        Err(invalid(loc, "expected an object"))
    }
}
pub fn obj(value: Value, r#where: String) -> Result<Value, String> {
    object(&value, &r#where)?;
    Ok(value)
}
fn items<'a>(value: &'a Value, loc: &str, max: i64) -> Result<&'a [Value], String> {
    value
        .as_array()
        .filter(|a| i64::try_from(a.len()).is_ok_and(|n| n <= max))
        .map(Vec::as_slice)
        .ok_or_else(|| {
            invalid(
                loc,
                &format!("expected an array with at most {max} entries"),
            )
        })
}
fn optional<'a>(value: Option<&'a Value>, loc: &str, max: i64) -> Result<&'a [Value], String> {
    value.map_or(Ok(&[]), |v| items(v, loc, max))
}
pub fn array(value: Value, r#where: String, maximum: i64) -> Result<Vec<Value>, String> {
    Ok(items(&value, &r#where, maximum)?.to_vec())
}
fn ws(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')
}
fn text<'a>(v: &'a Value, loc: &str, empty: bool) -> Result<&'a str, String> {
    v.as_str()
        .filter(|s| !s.contains('\0') && (empty || !s.chars().all(ws)))
        .ok_or_else(|| {
            invalid(
                loc,
                if empty {
                    "expected a valid string"
                } else {
                    "expected a valid string that is not empty"
                },
            )
        })
}
pub fn string(value: Value, r#where: String, empty: bool) -> Result<String, String> {
    text(&value, &r#where, empty).map(str::to_owned)
}
pub fn text_encoding(value: Value, r#where: String) -> Result<String, String> {
    let label = text(&value, &r#where, false)?;
    if encoding::resolve(label).is_some() {
        Ok(label.into())
    } else {
        Err(invalid(
            &r#where,
            "expected a supported text encoding, not a binary transform",
        ))
    }
}
fn bounded(v: &Value, loc: &str, min: i64, max: i64) -> Result<i64, String> {
    v.as_i64()
        .filter(|n| (min..=max).contains(n))
        .ok_or_else(|| invalid(loc, &format!("expected an integer in {min}..{max}")))
}
pub fn integer(
    value: Value,
    r#where: String,
    minimum: i64,
    maximum: Option<i64>,
) -> Result<i64, String> {
    bounded(&value, &r#where, minimum, maximum.unwrap_or(MAX_TIME_MS))
}
fn flag(v: &Value, loc: &str) -> Result<bool, String> {
    v.as_bool()
        .ok_or_else(|| invalid(loc, "expected a boolean"))
}
pub fn boolean(value: Value, r#where: String) -> Result<bool, String> {
    flag(&value, &r#where)
}
fn finite(v: Option<f64>, loc: &str) -> Result<f64, String> {
    v.filter(|n| (0.0..=1e12).contains(n))
        .ok_or_else(|| invalid(loc, "expected a finite nonnegative number"))
}
pub fn number(value: f64, r#where: String) -> Result<f64, String> {
    finite(Some(value), &r#where)
}
fn pattern<'a>(
    v: &'a Value,
    valid: impl FnOnce(&str) -> bool,
    loc: &str,
) -> Result<&'a str, String> {
    v.as_str()
        .filter(|s| valid(s))
        .ok_or_else(|| invalid(loc, "invalid identifier or digest"))
}
pub fn matched(value: Value, pattern: Value, r#where: String) -> Result<String, String> {
    let Some(s) = value.as_str() else {
        return Err(invalid(&r#where, "invalid identifier or digest"));
    };
    let Some(re) = pattern.get("$regex").and_then(Value::as_str) else {
        return Err(format!(
            "AttributeError: '{}' object has no attribute 'fullmatch'",
            python_type(&pattern)
        ));
    };
    let flags = pattern.get("flags").and_then(Value::as_i64).unwrap_or(0);
    let opts = [(2, 'i'), (8, 'm'), (16, 's'), (64, 'x')]
        .into_iter()
        .filter_map(|(mask, c)| (flags & mask != 0).then_some(c))
        .collect::<String>();
    let re = fancy_regex::Regex::new(&format!(r"\A(?{opts}:{re})\z"))
        .map_err(|_| invalid(&r#where, "unsupported regular expression"))?;
    if re
        .is_match(s)
        .map_err(|_| invalid(&r#where, "regular expression match exceeded its budget"))?
    {
        Ok(s.into())
    } else {
        Err(invalid(&r#where, "invalid identifier or digest"))
    }
}
fn python_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Array(_) => "list",
        Value::String(_) => "str",
        Value::Number(n) if n.is_f64() => "float",
        Value::Number(_) => "int",
        Value::Object(_) => "dict",
    }
}
pub fn glossary(value: Value, r#where: String) -> Result<Value, String> {
    records::glossary(&value, &r#where)?;
    Ok(value)
}
pub fn project(value: Value) -> Result<Value, String> {
    records::project(&value)?;
    Ok(value)
}
pub fn worksheet(value: Value, r#where: String) -> Result<Vec<Value>, String> {
    records::worksheet(&value, &r#where)?;
    array(value, r#where, 100000)
}
pub fn frame(value: Value, r#where: String, review: bool) -> Result<Value, String> {
    records::frame(&value, &r#where, review)?;
    Ok(value)
}
pub fn render_record(value: Value, r#where: String) -> Result<Value, String> {
    records::render_record(&value, &r#where)?;
    Ok(value)
}
pub fn fingerprint(value: Value, r#where: String, maximum: Option<i64>) -> Result<Value, String> {
    records::fingerprint(&value, &r#where, maximum.unwrap_or(16 * 1024_i64.pow(3)))?;
    Ok(value)
}
