//! Subtitle markup and logical bidi normalization.

#![forbid(unsafe_code)]

use fancy_regex::Regex as FancyRegex;
use regex::Regex;
use serde_json::{Value, json};
use std::collections::HashMap;
use unicode_bidi::BidiClass;

macro_rules! pattern {
    ($engine:ty, $source:expr) => {{
        static PATTERN: std::sync::OnceLock<$engine> = std::sync::OnceLock::new();
        // INFALLIBLE: Each pattern is built from static literals, exercised by the port tests.
        PATTERN.get_or_init(|| <$engine>::new($source).expect("valid subtitle pattern"))
    }};
}

mod bidi;
mod font;
mod tokens;

use tokens::{
    Piece::{Drawing, Tag, Text},
    block_spans, drawing_in, hidden, overrides, pieces,
};

fn regex_error(error: fancy_regex::Error) -> String {
    format!("RuntimeError: {error}")
}

fn python_space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')
}

fn tag_fits(tag: &str) -> bool {
    let prefix = if tag.starts_with("</") {
        2
    } else {
        tag.chars().next().map_or(0, char::len_utf8)
    };
    tag.len()
        .saturating_sub(prefix + tag.chars().next_back().map_or(0, char::len_utf8))
        <= 127
}

pub fn srt_tag_fits(tag: String) -> Result<bool, String> {
    Ok(tag_fits(&tag))
}

pub fn has_rtl(text: String) -> Result<bool, String> {
    Ok(bidi::has_rtl(&text))
}

pub fn has_ltr(text: String) -> Result<bool, String> {
    Ok(bidi::has_ltr(&text))
}

pub fn has_rtl_with_classes(text: &str, classes: &HashMap<char, BidiClass>) -> bool {
    bidi::has_rtl_with_classes(text, Some(classes))
}

pub fn has_ltr_with_classes(text: &str, classes: &HashMap<char, BidiClass>) -> bool {
    bidi::has_ltr_with_classes(text, Some(classes))
}

pub fn srt_font_names(tag: String) -> Result<Value, String> {
    srt_font_names_with_legacy_attributes(tag, false)
}

pub fn srt_font_names_with_legacy_attributes(
    tag: String,
    legacy_attributes: bool,
) -> Result<Value, String> {
    Ok(json!({"$set": font::names(&tag, legacy_attributes)?}))
}

pub fn drawing_mode(block: String, drawing: bool) -> Result<bool, String> {
    drawing_in(&block, drawing)
}

pub fn uncomment(text: String, kind: String) -> Result<String, String> {
    let (mut result, mut cursor, mut drawing) = (String::with_capacity(text.len()), 0, false);
    let (mut boundaries, mut active) = (Vec::new(), Vec::new());
    for span in block_spans(&text, &kind) {
        let (range, next) = span?;
        result.push_str(&text[cursor..range.start]);
        cursor = range.end;
        let block = &text[range];
        let remove = if kind == "srt" {
            hidden(block, &kind)
        } else {
            !block.contains('\\')
        };
        if remove {
            if kind == "srt" {
                boundaries.push(result.len());
            }
            if drawing {
                result.push_str("{}");
            }
        } else {
            result.push_str(block);
            if kind == "srt" {
                active.push(block);
            }
        }
        drawing = next;
    }
    result.push_str(&text[cursor..]);
    if !boundaries.is_empty() {
        const ACTIVATED: &str = "ValueError: SRT comment cleanup would activate literal markup; adapt the reviewed text explicitly";
        let mut boundaries = boundaries.into_iter().peekable();
        for tag in pattern!(Regex, r"</?[^<>]{0,127}>").find_iter(&result) {
            while boundaries.peek().is_some_and(|&p| p <= tag.start()) {
                boundaries.next();
            }
            if boundaries.peek().is_none_or(|&p| p >= tag.end()) || !tag_fits(tag.as_str()) {
                continue;
            }
            let closing = tag.as_str().starts_with("</");
            let body = &tag.as_str()[if closing { 2 } else { 1 }..tag.as_str().len() - 1];
            let name = body
                .trim_start_matches(' ')
                .split(' ')
                .next()
                .unwrap_or_default();
            let known = matches!(
                name.to_ascii_lowercase().as_str(),
                "font" | "b" | "i" | "u" | "s" | "br" | "br/"
            );
            let likely = name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'/'))
                && !body.starts_with(' ')
                && (closing || tag.start() == 0 || result.as_bytes()[tag.start() - 1] != b'<');
            if known || likely {
                return Err(ACTIVATED.into());
            }
        }
        let mut current = Vec::new();
        for span in block_spans(&result, &kind) {
            let (range, _) = span?;
            if !hidden(&result[range.clone()], &kind) {
                current.push(&result[range]);
            }
        }
        if current != active {
            return Err(ACTIVATED.into());
        }
    }
    Ok(result)
}

pub fn remap_resets(text: String, mapping: Value) -> Result<String, String> {
    let (mut result, mut cursor) = (String::with_capacity(text.len()), 0);
    for span in block_spans(&text, "ass") {
        let (range, _) = span?;
        result.push_str(&text[cursor..range.start]);
        cursor = range.end;
        let mut block = text[range].to_owned();
        let mut replacements = Vec::new();
        overrides(&block, |name, range| {
            if name == "r" && !range.is_empty() {
                let value = mapping
                    .get(&block[range.clone()])
                    .ok_or("ValueError: Donor reset references an undefined style")?;
                replacements.push((range, value));
            }
            Ok(())
        })?;
        for (range, value) in replacements.into_iter().rev() {
            let value = value.as_str().ok_or_else(|| {
                format!(
                    "TypeError: can only concatenate str (not \"{}\") to str",
                    python_type(value)
                )
            })?;
            block.replace_range(range, value);
        }
        result.push_str(&block);
    }
    result.push_str(&text[cursor..]);
    Ok(result)
}

fn python_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_f64() => "float",
        Value::Number(_) => "int",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(map) if map.contains_key("$int") => "int",
        Value::Object(map) if map.contains_key("$num") => "float",
        Value::Object(map) if map.contains_key("$set") => "set",
        Value::Object(map) if map.contains_key("$tuple") => "tuple",
        Value::Object(map) if map.contains_key("$bytes") => "bytes",
        Value::Object(_) => "dict",
    }
}

pub fn validate_bidi(text: String, kind: String) -> Result<(), String> {
    bidi::validate(&text, &kind)
}

/// `direction` defaults to `"rtl"` and `force` defaults to `False` in Python.
pub fn rtl(text: String, kind: String, direction: String, force: bool) -> Result<String, String> {
    bidi::normalize(&text, &kind, &direction, force, None)
}

pub fn rtl_with_classes(
    text: &str,
    kind: &str,
    direction: &str,
    force: bool,
    classes: &HashMap<char, BidiClass>,
) -> Result<String, String> {
    bidi::normalize(text, kind, direction, force, Some(classes))
}

/// `preserve` defaults to `False` in Python.
pub fn clean_empty_lines(text: String, kind: String, preserve: bool) -> Result<String, String> {
    let mut tokens = Vec::new();
    pieces(&text, &kind, |piece, value| {
        tokens.push((piece, value));
        Ok(())
    })?;
    if preserve || tokens.iter().any(|&(piece, _)| piece == Drawing) {
        return Ok(text);
    }
    if kind == "ass" {
        for &(_, block) in tokens.iter().filter(|&&(piece, _)| piece == Tag) {
            let mut karaoke = false;
            overrides(block, |name, _| {
                karaoke |= matches!(name, "k" | "K" | "kf" | "ko" | "kt");
                Ok(())
            })?;
            if karaoke {
                return Ok(text);
            }
        }
    }
    let delimiter = if kind == "ass" { r"\N" } else { "\n" };
    let (mut lines, mut line) = (Vec::new(), Vec::new());
    for (piece, value) in tokens {
        if piece != Text {
            line.push((piece, value));
            continue;
        }
        for (index, part) in value.split(delimiter).enumerate() {
            if index > 0 {
                lines.push(std::mem::take(&mut line));
            }
            line.push((piece, part));
        }
    }
    lines.push(line);
    let (mut result, mut pending, mut retained) = (String::new(), String::new(), false);
    for line in lines {
        let mut prose = line
            .iter()
            .filter_map(|&(piece, value)| (piece == Text).then_some(value))
            .collect::<String>();
        if kind == "ass" {
            prose = prose.replace(r"\n", " ").replace(r"\h", " ");
        }
        if prose
            .chars()
            .any(|c| !bidi::CONTROLS.contains(c) && !python_space(c))
        {
            if retained {
                result.push_str(delimiter);
            }
            retained = true;
            result.push_str(&pending);
            pending.clear();
            for (_, value) in line {
                result.push_str(value);
            }
        } else {
            for (piece, value) in line {
                if piece == Tag {
                    pending.push_str(value);
                } else {
                    pending.extend(value.chars().filter(|&c| bidi::CONTROLS.contains(c)));
                }
            }
        }
    }
    result.push_str(&pending);
    Ok(result)
}

pub fn paragraph_direction(language: String) -> Result<String, String> {
    let mut parts = language.split('-');
    let first = parts.next().unwrap_or_default().to_ascii_lowercase();
    let mut rtl = "fa fas pes prs ar ara he heb ur urd ps pus dv div yi yid ckb sd ug"
        .split_ascii_whitespace()
        .any(|language| language == first);
    for part in std::iter::once(first.as_str()).chain(parts) {
        if part.eq_ignore_ascii_case("latn") {
            return Ok("ltr".into());
        }
        rtl |= part.eq_ignore_ascii_case("arab") || part.eq_ignore_ascii_case("hebr");
    }
    Ok(if rtl { "rtl" } else { "ltr" }.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn counts_utf8_bytes_only_inside_the_tag() {
        assert_eq!(srt_tag_fits(format!("</{}a>", "é".repeat(63))), Ok(true));
        assert_eq!(srt_tag_fits(format!("</{}>", "é".repeat(64))), Ok(false));
    }

    #[test]
    fn finds_bidi_classes_beyond_arabic_and_latin() {
        assert_eq!(has_rtl("\u{200f}×²".into()), Ok(false));
        assert_eq!(has_rtl("\u{1e900}".into()), Ok(true));
        assert_eq!(has_ltr("漢".into()), Ok(true));
        assert_eq!(has_ltr("\u{200e}\u{0378}\u{1171e}".into()), Ok(false));
        assert_eq!(has_ltr("\u{1d6c1}".into()), Ok(true));
    }

    #[test]
    fn decodes_duplicate_font_attributes_without_reading_comments() {
        let tag =
            r#"<!--<font face=hidden>--><FONT face=" A&amp;B " face='C&#x2b;D' FACE="A&amp;B"/>"#;
        assert_eq!(
            srt_font_names(tag.into()),
            Ok(json!({"$set": ["A&B", "C+D"]}))
        );
        assert_eq!(
            srt_font_names(r#"<font face="&fjlig; &NotEqualTilde; &amp=">"#.into()),
            Ok(json!({"$set": ["fj \u{2242}\u{338} &amp="]}))
        );
    }

    #[test]
    fn legacy_python_attribute_entities_keep_their_prefix_contract() {
        assert_eq!(
            srt_font_names_with_legacy_attributes(
                r#"<font face="&fjlig; &amp= &notit; &unknown;">"#.into(),
                true,
            ),
            Ok(json!({"$set": ["fj &= ¬it; &unknown;"]}))
        );
    }

    #[test]
    fn scans_nested_drawings_and_rejects_bad_modes() {
        assert_eq!(drawing_mode(r"{\t(0,1,\p+0002)}".into(), false), Ok(true));
        assert_eq!(
            drawing_mode(r"{\p-1}".into(), false),
            Err("ValueError: ASS drawing mode must be a nonnegative integer".into())
        );
    }

    #[test]
    fn refuses_to_make_markup_by_removing_comments() {
        assert_eq!(
            uncomment("<fo<!--hidden-->nt>".into(), "srt".into()),
            Err(
                "ValueError: SRT comment cleanup would activate literal markup; adapt the reviewed text explicitly".into()
            )
        );
        assert_eq!(
            uncomment(r"{\p1}m 0 0{hidden}l 1 1".into(), "ass".into()).as_deref(),
            Ok(r"{\p1}m 0 0{}l 1 1")
        );
        assert_eq!(
            uncomment("a<br>b".into(), "other".into()).as_deref(),
            Ok("ab")
        );
    }

    #[test]
    fn replaces_only_the_selected_scalar_in_a_transform() {
        let text = r"{\t(\r( , Old ,ignored))}Hello";
        assert_eq!(
            remap_resets(text.into(), json!({"Old": "جدید"})).as_deref(),
            Ok(r"{\t(\r( , جدید ,ignored))}Hello")
        );
        assert_eq!(
            remap_resets(text.into(), json!({})),
            Err("ValueError: Donor reset references an undefined style".into())
        );
    }

    #[test]
    fn validates_isolates_without_reading_comment_controls() {
        assert_eq!(
            validate_bidi(
                "<!--\u{202c}-->\u{2066}\u{202a}site\u{2069}".into(),
                "srt".into()
            ),
            Ok(())
        );
        assert_eq!(
            validate_bidi("\u{2066}\u{202c}".into(), "srt".into()),
            Err("ValueError: Unbalanced bidi embedding terminator".into())
        );
    }

    #[test]
    fn preserves_cross_line_isolates_without_adding_embeddings() {
        let text = "\u{2066}سلام a\\Nسلام b\u{2069}";
        let normalized = "\u{200f}\u{2066}سلام a\u{200f}\\N\u{200f}سلام b\u{2069}\u{200f}";
        assert_eq!(
            rtl(text.into(), "ass".into(), "rtl".into(), false).as_deref(),
            Ok(normalized)
        );
        assert_eq!(
            rtl(normalized.into(), "ass".into(), "rtl".into(), false).as_deref(),
            Ok(normalized)
        );
    }

    #[test]
    fn keeps_karaoke_and_moves_tags_from_empty_lines() {
        let karaoke = r"{\k2}\NHello";
        assert_eq!(
            clean_empty_lines(karaoke.into(), "ass".into(), false).as_deref(),
            Ok(karaoke)
        );
        assert_eq!(
            clean_empty_lines("<i>\n  \nhello\n</i>".into(), "srt".into(), false).as_deref(),
            Ok("<i>hello</i>")
        );
    }

    #[test]
    fn latin_script_overrides_language_and_other_script_subtags() {
        assert_eq!(
            paragraph_direction("fa-Arab-LATN".into()).as_deref(),
            Ok("ltr")
        );
        assert_eq!(paragraph_direction("und-HeBR".into()).as_deref(), Ok("rtl"));
    }
}
