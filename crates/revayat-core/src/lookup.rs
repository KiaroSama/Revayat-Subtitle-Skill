//! Matches terminology from the bundled English and Persian banks.

#![forbid(unsafe_code)]

use fancy_regex::Regex;
use serde_json::{Value, json};
use std::{collections::HashSet, fs, path::Path};

const DATA: &str = "voice/persian/scripts/data";
const SUFFIX: &str = "(?:es|s|ed|ing|d)?";
// Python's word characters are Unicode letters, numbers, and underscore.
const BOUNDARY: &str = r"(?:(?<![\pL\pN_])(?=[\pL\pN_])|(?<=[\pL\pN_])(?![\pL\pN_]))";

fn space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')
}

fn trim(text: &str) -> &str {
    text.trim_matches(space)
}

fn marker<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.as_object().filter(|map| map.len() == 1)?.get(key)
}

fn py_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_f64() => "float",
        Value::Number(_) => "int",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        _ if marker(value, "$float").is_some() || marker(value, "$num").is_some() => "float",
        _ if marker(value, "$int").is_some() => "int",
        _ if marker(value, "$tuple").is_some() => "tuple",
        _ if marker(value, "$bytes").is_some() => "bytes",
        _ if marker(value, "$set").is_some() => "set",
        _ => "dict",
    }
}

fn items(value: &Value) -> Option<Vec<Value>> {
    match py_type(value) {
        "str" => Some(value.as_str()?.chars().map(|c| json!(c)).collect()),
        "list" => value.as_array().cloned(),
        "dict" => match marker(value, "$dict") {
            Some(pairs) => Some(
                pairs
                    .as_array()?
                    .iter()
                    .map(|pair| pair[0].clone())
                    .collect(),
            ),
            None => Some(value.as_object()?.keys().map(|key| json!(key)).collect()),
        },
        "tuple" => marker(value, "$tuple")?.as_array().cloned(),
        "bytes" => marker(value, "$bytes")?.as_array().cloned(),
        "set" => marker(value, "$set")?.as_array().cloned(),
        _ => None,
    }
}

fn iterable(value: &Value) -> Result<Vec<Value>, String> {
    items(value).ok_or_else(|| format!("TypeError: '{}' object is not iterable", py_type(value)))
}

fn triple(row: &Value) -> Result<[Value; 3], String> {
    let row = items(row).ok_or_else(|| {
        format!(
            "TypeError: cannot unpack non-iterable {} object",
            py_type(row)
        )
    })?;
    row.try_into().map_err(|row: Vec<Value>| match row.len() {
        count if count < 3 => {
            format!("ValueError: not enough values to unpack (expected 3, got {count})")
        }
        _ => "ValueError: too many values to unpack (expected 3)".into(),
    })
}

fn truthy(value: &Value) -> bool {
    match py_type(value) {
        "NoneType" => false,
        "bool" | "int" | "float" => number_key(value) != "0",
        _ => items(value).is_none_or(|items| !items.is_empty()),
    }
}

pub fn load(fname: serde_json::Value) -> Result<serde_json::Value, String> {
    let fname = fname.as_str().ok_or_else(|| match py_type(&fname) {
        "bytes" => "TypeError: Can't mix strings and bytes in path components".into(),
        kind => format!(
            "TypeError: join() argument must be str, bytes, or os.PathLike object, not '{kind}'"
        ),
    })?;
    let path = if Path::new(fname).is_absolute() {
        Some(Path::new(fname).to_path_buf())
    } else {
        crate::runtime::assets::find_file(&Path::new(DATA).join(fname))
    };
    if let Some(path) = path.filter(|path| path.exists()) {
        return fs::read_to_string(&path)
            .map(|text| bank_rows(&text))
            .map_err(|error| {
                let (kind, errno, detail) = match error.kind() {
                    std::io::ErrorKind::PermissionDenied => {
                        ("PermissionError", 13, "Permission denied")
                    }
                    std::io::ErrorKind::IsADirectory => ("IsADirectoryError", 21, "Is a directory"),
                    std::io::ErrorKind::NotFound => {
                        ("FileNotFoundError", 2, "No such file or directory")
                    }
                    _ => return format!("OSError: {error}"),
                };
                let path = path.to_string_lossy();
                let quote = if path.contains('\'') && !path.contains('"') {
                    '"'
                } else {
                    '\''
                };
                let escaped = path
                    .replace('\\', "\\\\")
                    .replace(quote, &format!("\\{quote}"));
                format!("{kind}: [Errno {errno}] {detail}: {quote}{escaped}{quote}")
            });
    }
    let built_in = match fname {
        "idioms.tsv" => {
            include_str!("../../../skills/revayat-subtitle/voice/persian/scripts/data/idioms.tsv")
        }
        "glossary.tsv" => {
            include_str!("../../../skills/revayat-subtitle/voice/persian/scripts/data/glossary.tsv")
        }
        _ => "",
    };
    Ok(bank_rows(built_in))
}

fn bank_rows(content: &str) -> Value {
    let rows: Vec<_> = content
        .split(['\r', '\n'])
        .filter(|line| !line.trim_start_matches(space).starts_with('#'))
        .filter_map(|line| {
            let mut parts = line.split('\t').map(trim);
            let (en, fa) = (parts.next()?, parts.next()?);
            (!en.is_empty() && !fa.is_empty())
                .then(|| json!({"$tuple": [en, fa, parts.next().unwrap_or_default()]}))
        })
        .collect();
    json!(rows)
}

fn pattern_parts(phrase: &Value) -> Result<(Vec<&str>, bool, bool), String> {
    let text = phrase.as_str().ok_or_else(|| {
        format!(
            "AttributeError: '{}' object has no attribute 'strip'",
            py_type(phrase)
        )
    })?;
    let words: Vec<_> = text.split(space).filter(|s| !s.is_empty()).collect();
    let last = words.last().copied().unwrap_or_default();
    let case_sensitive = last.chars().any(char::is_uppercase)
        && last
            .chars()
            .all(|c| c.is_uppercase() || (!c.is_lowercase() && c.to_lowercase().eq([c])));
    Ok((
        words,
        case_sensitive,
        !case_sensitive && last.chars().count() > 2,
    ))
}

pub fn en_pattern(phrase: serde_json::Value) -> Result<serde_json::Value, String> {
    let (words, exact, inflect) = pattern_parts(&phrase)?;
    let words: Vec<String> = words
        .into_iter()
        .map(|word| {
            let mut escaped = String::new();
            for c in word.chars() {
                if "()[]{}?*+-|^$\\.&~#".contains(c) {
                    escaped.push('\\');
                }
                escaped.push(c);
            }
            escaped
        })
        .collect();
    let suffix = if inflect { SUFFIX } else { "" };
    let pattern = format!(r"\b{}{suffix}\b", words.join(r"\s+"));
    Ok(json!({"$pattern": [pattern, if exact { 0 } else { 2 }]}))
}

fn compile_matcher(phrase: &Value) -> Result<Regex, String> {
    let (words, exact, inflect) = pattern_parts(phrase)?;
    let words: Vec<_> = words
        .into_iter()
        .map(|word| {
            if exact {
                return regex::escape(word);
            }
            word.chars()
                .map(|c| match c {
                    'I' | 'i' | 'İ' | 'ı' => "[Iiİı]".into(),
                    _ => regex::escape(c.encode_utf8(&mut [0; 4])),
                })
                .collect::<String>()
        })
        .collect();
    let suffix = if inflect {
        SUFFIX.replace('i', "[Iiİı]")
    } else {
        String::new()
    };
    let flags = if exact { "" } else { "(?i)" };
    Regex::new(&format!(
        "{flags}{BOUNDARY}{}{suffix}{BOUNDARY}",
        words.join(" +")
    ))
    .map_err(|error| format!("RuntimeError: {error}"))
}

pub fn find_en(
    rows: serde_json::Value,
    text: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let text = text.as_str().ok_or_else(|| match py_type(&text) {
        "bytes" => "TypeError: cannot use a string pattern on a bytes-like object".into(),
        kind => format!("TypeError: expected string or bytes-like object, got '{kind}'"),
    })?;
    let normalized = text
        .split(space)
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    // shortcut: narrow ASCII literals only; widen after rechecking the regex fold and error bounds.
    let folded = (normalized.len() <= 64 * 1024).then(|| {
        normalized
            .chars()
            .map(|c| match c {
                'İ' | 'ı' => 'i',
                'K' => 'k',
                'ſ' => 's',
                _ => c.to_ascii_lowercase(),
            })
            .collect::<String>()
    });
    let mut hits = Vec::new();
    for row in iterable(&rows)? {
        let [en, fa, note] = triple(&row)?;
        if let (Some(folded), Some(phrase)) = (&folded, en.as_str())
            && phrase.len() <= 128
            && phrase
                .trim_matches(space)
                .bytes()
                .all(|byte| byte.is_ascii_graphic() || byte.is_ascii_whitespace())
        {
            // Normalization leaves only one space per separator, so there is no branching across words.
            let mut words = phrase.split(space).filter(|word| !word.is_empty());
            if words.clone().count() <= 6
                && words.any(|word| !folded.contains(&word.to_ascii_lowercase()))
            {
                continue;
            }
        }
        if compile_matcher(&en)?
            .is_match(&normalized)
            .map_err(|error| format!("RuntimeError: {error}"))?
        {
            hits.push(json!({"$tuple": [en, fa, note]}));
        }
    }
    Ok(json!(hits))
}

pub fn find_fa(
    rows: serde_json::Value,
    text: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut hits = Vec::new();
    for row in iterable(&rows)? {
        let [en, fa, note] = triple(&row)?;
        if !truthy(&fa) {
            continue;
        }
        let translation = fa.as_str().ok_or_else(|| match py_type(&fa) {
            "bytes" => "TypeError: a bytes-like object is required, not 'str'".into(),
            kind => format!("AttributeError: '{kind}' object has no attribute 'split'"),
        })?;
        let phrase = trim(
            translation
                .split_once('/')
                .map_or(translation, |(first, _)| first),
        );
        if phrase.is_empty() {
            continue;
        }
        let found = match py_type(&text) {
            "str" => text.as_str().is_some_and(|s| s.contains(phrase)),
            "bytes" => return Err("TypeError: a bytes-like object is required, not 'str'".into()),
            kind => items(&text)
                .ok_or_else(|| format!("TypeError: argument of type '{kind}' is not iterable"))?
                .iter()
                .any(|item| item.as_str() == Some(phrase)),
        };
        if found {
            hits.push(json!({"$tuple": [en, fa, note]}));
        }
    }
    Ok(json!(hits))
}

fn number_key(value: &Value) -> String {
    if let Some(b) = value.as_bool() {
        return u8::from(b).to_string();
    }
    if let Some(i) = marker(value, "$int").and_then(Value::as_str) {
        return i.into();
    }
    if let Some(n) = value.as_number().filter(|n| !n.is_f64()) {
        return n.to_string();
    }
    let float = value.as_f64().or_else(|| {
        marker(value, "$float")
            .or_else(|| marker(value, "$num"))?
            .as_str()?
            .parse()
            .ok()
    });
    match float {
        Some(0.0) => "0".into(),
        Some(x) if x.is_finite() && x.fract() == 0.0 => format!("{x:.0}"),
        Some(x) => x.to_string(),
        None => value.to_string(),
    }
}

fn hashable(value: &Value) -> Result<Value, String> {
    match py_type(value) {
        "list" | "set" | "dict" => Err(format!("TypeError: unhashable type: '{}'", py_type(value))),
        "bool" | "float" | "int" => Ok(json!({"$number": number_key(value)})),
        "tuple" => {
            let values = value["$tuple"]
                .as_array()
                .into_iter()
                .flatten()
                .map(hashable)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(json!({"$tuple": values}))
        }
        _ => Ok(value.clone()),
    }
}

pub fn dedup(hits: serde_json::Value) -> Result<serde_json::Value, String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for hit in iterable(&hits)? {
        let key = match py_type(&hit) {
            "dict" => marker(&hit, "$dict")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|pair| {
                    matches!(py_type(&pair[0]), "bool" | "float" | "int")
                        && number_key(&pair[0]) == "0"
                })
                .map(|pair| pair[1].clone())
                .ok_or("KeyError: 0")?,
            kind @ ("str" | "list" | "tuple" | "bytes") => items(&hit)
                .and_then(|items| items.into_iter().next())
                .ok_or_else(|| {
                    let kind = if kind == "str" { "string" } else { kind };
                    format!("IndexError: {kind} index out of range")
                })?,
            kind => return Err(format!("TypeError: '{kind}' object is not subscriptable")),
        };
        if seen.insert(hashable(&key)?.to_string()) {
            out.push(hit);
        }
    }
    Ok(json!(out))
}

/// Return both terminology banks in the CLI's JSON shape.
pub fn lookup_text(text: &str, reverse: bool) -> Result<Value, String> {
    let mut result = serde_json::Map::new();
    let find = if reverse { find_fa } else { find_en };
    for (label, filename) in [("idioms", "idioms.tsv"), ("glossary", "glossary.tsv")] {
        let hits = dedup(find(load(json!(filename))?, json!(text))?)?;
        let rows = hits
            .as_array()
            .into_iter()
            .flatten()
            .map(|row| {
                let fields = &row["$tuple"];
                json!({"en": fields[0], "fa": fields[1], "note": fields[2]})
            })
            .collect();
        result.insert(label.into(), Value::Array(rows));
    }
    Ok(Value::Object(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_inflected_phrases_without_matching_inside_words() -> Result<(), String> {
        let rows = json!([["business partner", "شریک", ""], ["AI", "هوش", ""]]);
        let result = find_en(
            rows,
            json!("xAI ai Business\u{1c}Partners; xbusiness partner"),
        )?;
        assert_eq!(
            result,
            json!([{"$tuple": ["business partner", "شریک", ""]}])
        );
        Ok(())
    }

    #[test]
    fn english_lookup_preserves_exact_regex_matches_and_errors() -> Result<(), String> {
        let phrases = [
            "business partner",
            "office",
            "keep silent",
            "AI",
            "a+b (ok)",
            "Straße",
            "İstanbul",
            "\u{1c}",
            "foo bar",
        ];
        let rows = json!(phrases.map(|phrase| [phrase, "translation", "note"]));
        for text in [
            "Business\u{1c}Partners; offıce, OFFİCE; Keep ſilent; AI ai; a+b (ok)z STRAẞE İstanbul",
            "xoffice keep; silent; business; partner; \u{a0}foo\u{ad}bar\u{202f}",
            "",
        ] {
            let normalized = text
                .split(space)
                .filter(|word| !word.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            let mut expected = Vec::new();
            for phrase in phrases {
                if compile_matcher(&json!(phrase))?
                    .is_match(&normalized)
                    .map_err(|e| e.to_string())?
                {
                    expected.push(json!({"$tuple": [phrase, "translation", "note"]}));
                }
            }
            assert_eq!(find_en(rows.clone(), json!(text))?, json!(expected));
        }
        assert!(
            find_en(json!([[null, "translation", "note"]]), json!(""))
                .is_err_and(|error| error.contains("has no attribute 'strip'"))
        );
        let text = "x".repeat(1_000_001);
        assert!(
            find_en(json!([["absent", "translation", "note"]]), json!(text))
                .is_err_and(|error| error.contains("backtracking"))
        );
        Ok(())
    }

    #[test]
    fn reverse_matches_first_translation_and_dedup_keeps_first_hit() -> Result<(), String> {
        let rows = json!([["a", "سلام / درود", 1], ["a", "سلام", 2]]);
        let hits = find_fa(rows.clone(), json!("درود سلام"))?;
        assert_eq!(dedup(hits)?, json!([{"$tuple": ["a", "سلام / درود", 1]}]));
        assert_eq!(find_fa(rows, json!("درود"))?, json!([]));
        Ok(())
    }

    #[test]
    fn loads_the_bundled_bank() -> Result<(), String> {
        let rows = load(json!("idioms.tsv"))?;
        assert!(rows.as_array().is_some_and(|rows| !rows.is_empty()));
        assert_eq!(load(json!("not-a-term-bank.tsv"))?, json!([]));
        Ok(())
    }
}
