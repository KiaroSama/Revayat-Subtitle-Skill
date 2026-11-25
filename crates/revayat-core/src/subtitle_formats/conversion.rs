use super::{
    document::{self, Cue, Document},
    invalid, markup,
};
use crate::text_pipeline as text;
use indexmap::IndexMap;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

pub struct Options<'a> {
    pub style: Option<&'a str>,
    pub title: &'a str,
    pub direction: &'a str,
    pub fix_zwnj: bool,
    pub strip_watermarks: bool,
    pub wrap_lines: bool,
    pub max_chars: i64,
}
impl Default for Options<'_> {
    fn default() -> Self {
        Self {
            style: None,
            title: "",
            direction: "rtl",
            fix_zwnj: true,
            strip_watermarks: true,
            wrap_lines: true,
            max_chars: 42,
        }
    }
}

pub fn srt_to_ass(cue: Cue) -> Result<Cue, String> {
    let text = markup::remove_comments(cue.text.replace("\r\n", "\n").replace('\r', "\n"))?;
    markup::validate_donor(&text)?;
    if markup::contains_ass_control(&text)? {
        return Err(invalid(
            "SRT donor contains ASS control syntax; adapt it explicitly before ASS conversion",
        ));
    }
    let text = text::convert_srt_markup_to_ass(text)?;
    for tag in document::srt_tags(&text)? {
        if !markup::alignment(&tag)? {
            return Err(invalid(
                "SRT donor uses markup requiring explicit ASS adaptation",
            ));
        }
    }
    let fields = document::ASS_FIELDS
        .split(',')
        .map(|f| document::ass_key(f.trim()))
        .zip(["0", "", "", "Default", "", "0", "0", "0", "", ""].map(str::to_owned))
        .collect();
    let (start, end) = document::effective_times(cue.start, cue.end, "ass")?;
    Ok(Cue {
        start,
        end,
        text,
        fields,
        ..cue
    })
}
fn mark(doc: &Document, cue: &Cue, enabled: bool) -> Result<Option<String>, String> {
    if !enabled
        || markup::pieces(&cue.text, &doc.kind)?
            .iter()
            .any(|(k, v)| *k == "drawing" && !v.trim().is_empty())
    {
        return Ok(None);
    }
    let reason = text::watermark_reason_with_kind(cue.text.clone(), &doc.kind)?;
    Ok((!reason.is_empty()).then_some(reason))
}
fn process(
    doc: &Document,
    mut result: Document,
    style: &str,
    options: &Options<'_>,
) -> Result<(Document, Vec<Value>), String> {
    if !matches!(options.direction, "rtl" | "ltr") {
        return Err(invalid("Paragraph direction must be ltr or rtl"));
    }
    if options.max_chars < 1 {
        return Err(invalid(
            "Line wrapping requires positive max_chars and max_lines",
        ));
    }
    let mut removed = Vec::new();
    for source in &doc.cues {
        document::effective_times(source.start, source.end, "ass")?;
        if let Some(reason) = mark(doc, source, options.strip_watermarks)? {
            removed.push(json!({"id":source.id,"start_ms":source.start,"end_ms":source.end,"reason":reason,"text":source.text}));
            continue;
        }
        let mut cue = if doc.kind == "srt" {
            let mut cue = srt_to_ass(source.clone())?;
            cue.fields.insert("style".into(), style.into());
            cue
        } else {
            source.clone()
        };
        if options.direction == "rtl" && options.fix_zwnj {
            cue.text = text::normalize_persian_zwnj(cue.text)?;
        }
        if options.wrap_lines {
            cue.text = text::wrap_subtitle_lines(cue.text, Some(options.max_chars), Some(2))?;
        }
        cue.text =
            text::anchor_rtl_line(cue.text, Some("ass".into()), Some(options.direction.into()))?;
        result.cues.push(cue);
    }
    Ok((result, removed))
}
pub fn convert(doc: &Document, options: &Options<'_>) -> Result<(Document, Vec<Value>), String> {
    match doc.kind.as_str() {
        "srt" => srt_document_to_ass(doc, 1920, 1080, options),
        "ass" => ass_document_to_ass(doc, options),
        _ => Err(invalid("Supported formats are .ass and .srt")),
    }
}

pub fn srt_document_to_ass(
    doc: &Document,
    x: i64,
    y: i64,
    options: &Options<'_>,
) -> Result<(Document, Vec<Value>), String> {
    if doc.kind != "srt" {
        return Err(invalid("SRT document conversion requires SRT input"));
    }
    let result = document::ass_template(options.style, options.title, x, y)?;
    let style = result
        .styles
        .keys()
        .next()
        .cloned()
        .ok_or_else(|| invalid("ASS style not found"))?;
    process(doc, result, &style, options)
}
pub fn ass_document_to_ass(
    doc: &Document,
    options: &Options<'_>,
) -> Result<(Document, Vec<Value>), String> {
    if doc.kind != "ass" {
        return Err(invalid("ASS document conversion requires ASS input"));
    }
    let template = document::ass_template(options.style, options.title, 1920, 1080)?;
    let (style, row) = template
        .styles
        .first()
        .ok_or_else(|| invalid("ASS style not found"))?;
    let by_field: HashMap<_, _> = template.style_fields.iter().zip(row).collect();
    let mut result = doc.clone();
    result.cues.clear();
    result.source_comments.clear();
    result.comments = 0;
    result.styles.insert(
        style.clone(),
        doc.style_fields
            .iter()
            .map(|f| by_field.get(f).map_or_else(String::new, |s| (*s).clone()))
            .collect(),
    );
    if !options.title.is_empty() {
        for (section, lines) in &mut result.sections {
            if document::ass_key(section) == "[script info]" {
                lines.retain(|line| {
                    !document::ass_key(document::ass_line_start(line)).starts_with("title:")
                });
                lines.insert(0, format!("Title: {}", options.title));
            }
        }
    }
    process(doc, result, style, options)
}
fn reference(label: &str, text: &str) -> String {
    format!("{label}: {text}")
        .replace("\r\n", "\n")
        .replace(['\r', '\n'], r"\N")
        .replace('\0', "")
}
pub use build_translator_ass as translator_ass;

pub fn build_translator_ass(
    primary_doc: &Document,
    donors: &[(String, Document)],
    include_comments: bool,
    options: &Options<'_>,
) -> Result<(Document, Value), String> {
    let (mut edition, removed) = convert(primary_doc, options)?;
    let raw = options.style.unwrap_or(document::DEFAULT_STYLE);
    let style = document::ass_style_name(
        raw.strip_prefix("Style:")
            .unwrap_or(raw)
            .split(',')
            .next()
            .unwrap_or(""),
        false,
    );
    let mut primary: Vec<_> = edition.cues.iter().collect();
    primary.sort_by_key(|c| c.start);
    let mut far_end = i64::MIN;
    let far_ends: Vec<_> = primary
        .iter()
        .map(|c| {
            far_end = far_end.max(c.end);
            far_end
        })
        .collect();
    let mut placeholders: IndexMap<(i64, i64), Cue> = IndexMap::new();
    let mut ids: HashSet<_> = primary.iter().map(|c| c.id.clone()).collect();
    if include_comments {
        let sources: HashMap<_, _> = primary_doc.cues.iter().map(|c| (&c.id, &c.text)).collect();
        for cue in &primary {
            let text = sources
                .get(&cue.id)
                .ok_or_else(|| String::from("KeyError: Missing primary cue"))?;
            edition
                .source_comments
                .insert(cue.id.clone(), vec![reference("Primary", text)]);
        }
    }
    let mut watermarks = removed.len();
    for (label, doc) in donors {
        for source in &doc.cues {
            let (start, end) = document::effective_times(source.start, source.end, "ass")?;
            if mark(doc, source, options.strip_watermarks)?.is_some() {
                watermarks += 1;
                continue;
            }
            let begin = far_ends.partition_point(|&n| n < start - 150);
            let finish = primary.partition_point(|c| c.start <= end + 150);
            let mut best = None;
            let mut score = -151;
            if let Some(candidates) = primary.get(begin..finish) {
                for candidate in candidates {
                    let overlap = end.min(candidate.end) - start.max(candidate.start);
                    if overlap > score
                        && (overlap * 4 > end - start || (-150..=0).contains(&overlap))
                    {
                        score = overlap;
                        best = Some(candidate.id.as_str());
                        if !include_comments {
                            break;
                        }
                    }
                }
            }
            let id = if let Some(id) = best {
                id
            } else {
                let count = placeholders.len() + 1;
                &placeholders
                    .entry((start, end))
                    .or_insert_with(|| {
                        let mut id = format!("slot-{count}");
                        while !ids.insert(id.clone()) {
                            id.push('-');
                        }
                        Cue {
                            id,
                            start,
                            end,
                            text: String::new(),
                            fields: IndexMap::from([
                                ("style".into(), style.into()),
                                ("layer".into(), "0".into()),
                                ("marginl".into(), "0".into()),
                                ("marginr".into(), "0".into()),
                                ("marginv".into(), "0".into()),
                            ]),
                        }
                    })
                    .id
            };
            if include_comments {
                edition
                    .source_comments
                    .entry(id.into())
                    .or_default()
                    .push(reference(label, &source.text));
            }
        }
    }
    let counts = json!({"primary_cues":primary.len(),"placeholder_slots":placeholders.len(),"watermarks_removed":watermarks,"style":style});
    edition.cues.extend(placeholders.into_values());
    edition.cues = document::presentation_order(&edition.cues, "ass")?
        .into_iter()
        .map(|i| edition.cues[i].clone())
        .collect();
    edition.comments = edition.source_comments.values().map(Vec::len).sum();
    edition.chronological = false;
    Ok((edition, counts))
}
