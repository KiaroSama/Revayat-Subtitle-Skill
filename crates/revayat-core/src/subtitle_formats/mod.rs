//! ASS and SRT parsing, timing, and presentation-preserving conversion.
#![forbid(unsafe_code)]
#![expect(
    clippy::too_many_arguments,
    reason = "Preserve the Python bridge signatures"
)]
use serde_json::{Value, json};
pub mod conversion;
pub mod document;
mod markup;
mod model;
#[cfg(test)]
mod tests;
mod timing;
use conversion::Options;
pub use conversion::{convert, translator_ass};
use document::{Cue, Document};
pub use document::{LineEnding, SubtitleCue, SubtitleDocument, SubtitleFormat, TimestampCs};

fn invalid(s: &str) -> String {
    format!("ValueError: {s}")
}
fn pytype(v: &Value) -> &'static str {
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
fn attr(v: &Value, name: &str) -> String {
    format!(
        "AttributeError: '{}' object has no attribute '{name}'",
        pytype(v)
    )
}
fn attrs<'a>(v: &'a Value, kind: &str, field: &str) -> Result<&'a Value, String> {
    if v["$object"].as_str() == Some(kind) {
        Ok(&v["attrs"])
    } else {
        Err(attr(v, field))
    }
}
fn cue(v: &Value, field: &str) -> Result<Cue, String> {
    serde_json::from_value(attrs(v, "Cue", field)?.clone())
        .map_err(|_| invalid("Invalid subtitle cue"))
}
fn doc(v: &Value, field: &str) -> Result<Document, String> {
    let mut a = attrs(v, "Document", field)?.clone();
    if let Some(cues) = a.get_mut("cues").and_then(Value::as_array_mut) {
        for c in cues {
            if c["$object"].as_str() == Some("Cue") {
                *c = std::mem::take(&mut c["attrs"]);
            }
        }
    }
    if let Some(sections) = a.get_mut("sections").and_then(Value::as_array_mut) {
        for s in sections {
            if let Some(tuple) = s.get_mut("$tuple") {
                *s = std::mem::take(tuple);
            }
        }
    }
    serde_json::from_value(a).map_err(|_| invalid("Invalid subtitle document"))
}
fn cue_value(cue: Cue) -> Value {
    json!({"$object":"Cue","attrs":cue})
}
fn doc_value(doc: Document) -> Value {
    let mut attrs = json!(&doc);
    attrs["sections"] = Value::Array(
        doc.sections
            .into_iter()
            .map(|(name, lines)| json!({"$tuple":[name,lines]}))
            .collect(),
    );
    attrs["cues"] = Value::Array(doc.cues.into_iter().map(cue_value).collect());
    json!({"$object":"Document","attrs":attrs})
}
fn general(n: f64) -> String {
    if n == 0.0 {
        return if n.is_sign_negative() { "-0" } else { "0" }.into();
    }
    let s = format!("{n:.5e}");
    let Some((mantissa, exp)) = s.split_once('e') else {
        return s;
    };
    let exponent = exp.parse::<i32>().unwrap_or(0);
    let trim = |s: String| {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            s
        }
    };
    if !(-4..6).contains(&exponent) {
        format!(
            "{}e{}{:02}",
            trim(mantissa.into()),
            if exponent < 0 { '-' } else { '+' },
            exponent.unsigned_abs()
        )
    } else {
        let decimals = (5 - exponent) as usize;
        trim(format!("{n:.decimals$}"))
    }
}
pub fn build_ass_style(
    name: String,
    fontname: Option<Value>,
    fontsize: i64,
    outline: Option<Value>,
    shadow: Option<Value>,
    outline_colour: Option<Value>,
    back_colour: Option<Value>,
    bold: Value,
    alignment: i64,
    margin_v: i64,
) -> Result<String, String> {
    let invalid_name = |v: &str| v.trim().is_empty() || v.contains([',', '\r', '\n', '\0']);
    let name_error = || {
        invalid(
            "ASS style and font names must be nonempty and cannot contain commas or line breaks",
        )
    };
    if invalid_name(&name) {
        return Err(name_error());
    }
    let font = fontname.as_ref().map_or(Ok("B Koodak"), |v| {
        v.as_str().ok_or_else(|| attr(v, "strip"))
    })?;
    if invalid_name(font) {
        return Err(name_error());
    }
    let number = |v: &Value| {
        v.as_f64()
            .or_else(|| v.as_bool().map(|b| if b { 1.0 } else { 0.0 }))
            .ok_or_else(|| format!("TypeError: must be real number, not {}", pytype(v)))
    };
    let font_size = fontsize as f64;
    let outline = outline.as_ref().map_or(Ok(1.5), number)?;
    let shadow = shadow.as_ref().map_or(Ok(0.8), number)?;
    if font_size <= 0.0 || [outline, shadow].iter().any(|n| !n.is_finite() || *n < 0.0) {
        return Err(invalid(
            "ASS font size must be positive and outline/shadow must be nonnegative finite numbers",
        ));
    }
    fn color(v: &Value) -> Result<&str, String> {
        let s = v.as_str().ok_or_else(|| {
            format!(
                "TypeError: expected string or bytes-like object, got '{}'",
                pytype(v)
            )
        })?;
        if s.strip_prefix("&H")
            .is_some_and(|s| s.len() == 8 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            Ok(s)
        } else {
            Err(invalid("ASS outline and shadow colours require &HAABBGGRR"))
        }
    }
    let outline_colour = outline_colour.as_ref().map_or(Ok("&H00000000"), color)?;
    let back_colour = back_colour.as_ref().map_or(Ok("&H80000000"), color)?;
    let bold = match (bold.as_i64(), bold.as_f64(), bold.as_bool()) {
        (Some(n @ (-1 | 0)), _, _) => n.to_string(),
        (_, Some(n), _) if n == 0.0 || n == -1.0 => format!("{n:.1}"),
        (_, _, Some(false)) => String::from("False"),
        _ => return Err(invalid("Invalid ASS bold, alignment or vertical margin")),
    };
    if !(1..=9).contains(&alignment) || margin_v < 0 {
        return Err(invalid("Invalid ASS bold, alignment or vertical margin"));
    }
    Ok(format!(
        "{name},{font},{},&H00FFFFFF,&H000000FF,{outline_colour},{back_colour},{bold},0,0,0,100,100,0,0,1,{},{},{alignment},20,20,{margin_v},1",
        general(font_size),
        general(outline),
        general(shadow)
    ))
}
pub fn ass_key(value: String) -> Result<String, String> {
    Ok(document::ass_key(&value))
}
pub fn ass_line_start(value: String) -> Result<String, String> {
    Ok(document::ass_line_start(&value).into())
}
pub fn ass_style_name(value: String, reference: bool) -> Result<String, String> {
    Ok(document::ass_style_name(&value, reference).into())
}
pub fn timestamp(value: String, kind: String) -> Result<i64, String> {
    document::timestamp(&value, &kind)
}
pub fn timecode(ms: i64, kind: String) -> Result<String, String> {
    document::timecode(ms, &kind)
}
pub fn effective_times(start: i64, end: i64, kind: String) -> Result<Value, String> {
    let (s, e) = document::effective_times(start, end, &kind)?;
    Ok(json!({"$tuple":[s,e]}))
}
pub fn visible(text: String, kind: String) -> Result<String, String> {
    let mut s = markup::pieces(&text, &kind)?
        .into_iter()
        .filter_map(|(k, v)| (k == "text").then_some(v))
        .collect::<String>();
    if kind == "ass" {
        s = s
            .replace(r"\N", "\n")
            .replace(r"\n", " ")
            .replace(r"\h", " ")
            .replace(r"\{", "{")
            .replace(r"\}", "}");
    }
    s.retain(|c|!matches!(c,'\u{061c}'|'\u{200e}'|'\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'));
    Ok(
        s.trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'))
            .into(),
    )
}
pub fn structure(text: String, kind: String) -> Result<Vec<Value>, String> {
    Ok(markup::pieces(&text, &kind)?
        .into_iter()
        .filter(|(k, _)| *k != "text")
        .map(|(k, v)| json!({"$tuple":[k,v]}))
        .collect())
}
pub fn has_drawing(text: String, kind: String) -> Result<bool, String> {
    Ok(markup::pieces(&text, &kind)?
        .into_iter()
        .any(|(k, v)| k == "drawing" && !v.trim().is_empty()))
}
pub fn style_references(value: Value) -> Result<Value, String> {
    Ok(json!({"$set":document::style_references(&cue(&value,"fields")?)?}))
}
pub fn presentation_order(values: Vec<Value>, kind: String) -> Result<Vec<i64>, String> {
    if !matches!(kind.as_str(), "ass" | "srt") {
        return Err(invalid("Cue ordering requires ASS or SRT format"));
    }
    let cues = values
        .iter()
        .map(|v| cue(v, "start"))
        .collect::<Result<Vec<_>, _>>()?;
    document::presentation_order(&cues, &kind)?
        .into_iter()
        .map(|i| i64::try_from(i).map_err(|_| invalid("Cue index exceeds its range")))
        .collect()
}
pub fn serialize(value: Value, values: Vec<Value>, remove_fonts: bool) -> Result<String, String> {
    if values.is_empty() {
        return Err(invalid("Refusing an episode with no retained cues"));
    }
    let doc = doc(&value, "chronological")?;
    let cues = values
        .iter()
        .map(|v| cue(v, "start"))
        .collect::<Result<Vec<_>, _>>()?;
    document::serialize(&doc, &cues, remove_fonts)
}
pub fn validate_srt_donor_markup(text: String) -> Result<(), String> {
    markup::validate_donor(&text)
}
pub fn srt_to_ass(value: Value) -> Result<Value, String> {
    Ok(cue_value(document::srt_to_ass(cue(&value, "text")?)?))
}
pub fn ass_template(
    style_line: Option<String>,
    title: String,
    x: Option<i64>,
    y: Option<i64>,
) -> Result<Value, String> {
    Ok(doc_value(document::ass_template(
        style_line.as_deref(),
        &title,
        x.ok_or_else(|| invalid("ASS script resolution requires positive integers"))?,
        y.ok_or_else(|| invalid("ASS script resolution requires positive integers"))?,
    )?))
}
pub fn srt_document_to_ass(
    value: Value,
    style_line: Option<String>,
    title: String,
    x: Option<i64>,
    y: Option<i64>,
    direction: String,
    fix_zwnj: bool,
    strip_watermarks: bool,
    wrap_lines: bool,
    max_chars: i64,
) -> Result<Value, String> {
    let doc = doc(&value, "kind")?;
    if doc.kind != "srt" {
        return Err(invalid("SRT document conversion requires SRT input"));
    }
    let options = Options {
        style: style_line.as_deref(),
        title: &title,
        direction: &direction,
        fix_zwnj,
        strip_watermarks,
        wrap_lines,
        max_chars,
    };
    let (doc, removed) =
        conversion::srt_document_to_ass(&doc, x.unwrap_or(1920), y.unwrap_or(1080), &options)?;
    Ok(json!({"$tuple":[doc_value(doc),removed]}))
}
pub fn ass_document_to_ass(
    value: Value,
    style_line: Option<String>,
    title: String,
    direction: String,
    fix_zwnj: bool,
    strip_watermarks: bool,
    wrap_lines: bool,
    max_chars: i64,
) -> Result<Value, String> {
    let options = Options {
        style: style_line.as_deref(),
        title: &title,
        direction: &direction,
        fix_zwnj,
        strip_watermarks,
        wrap_lines,
        max_chars,
    };
    let (doc, removed) = conversion::ass_document_to_ass(&doc(&value, "kind")?, &options)?;
    Ok(json!({"$tuple":[doc_value(doc),removed]}))
}
pub fn build_translator_ass(
    primary: Value,
    donors: Option<Vec<Value>>,
    include_comments: bool,
    style_line: Option<String>,
    title: String,
    strip_watermarks: bool,
    direction: String,
    fix_zwnj: bool,
    wrap_lines: bool,
    max_chars: i64,
) -> Result<Value, String> {
    let primary = doc(&primary, "kind")?;
    let mut parsed = Vec::new();
    for donor in donors.unwrap_or_default() {
        let pair = donor
            .as_array()
            .or_else(|| donor["$tuple"].as_array())
            .filter(|pair| pair.len() == 2)
            .ok_or_else(|| invalid("Invalid donor document record"))?;
        let label = pair[0]
            .as_str()
            .ok_or_else(|| invalid("Invalid donor label"))?;
        parsed.push((label.to_owned(), doc(&pair[1], "cues")?));
    }
    let options = Options {
        style: style_line.as_deref(),
        title: &title,
        direction: &direction,
        fix_zwnj,
        strip_watermarks,
        wrap_lines,
        max_chars,
    };
    let (doc, counts) =
        conversion::build_translator_ass(&primary, &parsed, include_comments, &options)?;
    Ok(json!({"$tuple":[doc_value(doc),counts]}))
}
