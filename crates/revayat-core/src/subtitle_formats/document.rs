use super::model::line_ending;
pub use super::model::{
    Cue, Document, LineEnding, SubtitleCue, SubtitleDocument, SubtitleFormat, TimestampCs,
};
use super::{invalid, markup};
use indexmap::IndexMap;
use std::{collections::HashSet, fmt::Write};

pub const ASS_FIELDS: &str =
    "Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text";
pub const STYLE_FIELDS: &str = "Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding";
pub const DEFAULT_STYLE: &str = "Default,B Koodak,110,&H00FFFFFF,&H000000FF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1";
const NATIVE: [&str; 5] = [
    "[script info]",
    "[v4+ styles]",
    "[v4 styles]",
    "[events]",
    "[fonts]",
];
const HEADERS: [&str; 11] = [
    "ScriptType",
    "PlayResX",
    "PlayResY",
    "LayoutResX",
    "LayoutResY",
    "Timer",
    "WrapStyle",
    "ScaledBorderAndShadow",
    "Kerning",
    "YCbCr Matrix",
    "Language",
];
pub use super::timing::{effective_times, timecode, timestamp};

pub fn ass_key(s: &str) -> String {
    s.to_ascii_lowercase()
}
pub fn ass_line_start(s: &str) -> &str {
    s.trim_start_matches('\u{feff}')
        .trim_start_matches([' ', '\t'])
}
pub fn ass_style_name(s: &str, reference: bool) -> &str {
    let name = s.trim_start_matches([' ', '\t']).trim_start_matches('*');
    if if reference {
        name.eq_ignore_ascii_case("Default")
    } else {
        name.is_empty()
    } {
        "Default"
    } else {
        name
    }
}
fn fields(row: &str) -> Vec<String> {
    row.split(',')
        .map(|s| ass_key(s.trim_matches([' ', '\t'])))
        .collect()
}
fn parse_srt(data: &str, doc: &mut Document) -> Result<(), String> {
    let breaks =
        regex::Regex::new(r"\n(?:[ \t]*\n)+").map_err(|_| invalid("Invalid SRT cue separator"))?;
    let digits = regex::Regex::new(r"\A[\p{Nd}²³¹⁰⁴-⁹₀-₉]+\z")
        .map_err(|_| invalid("Invalid SRT cue numbering pattern"))?;
    for block in breaks.split(data.trim()) {
        if block.trim().is_empty() {
            continue;
        }
        let mut lines = block.splitn(3, '\n');
        let cue = lines.next().unwrap_or("").trim();
        let row = lines
            .next()
            .filter(|_| digits.is_match(cue))
            .ok_or_else(|| invalid("Malformed SRT cue; expected a number and timestamp line"))?;
        let (start, end) = row
            .split_once(" --> ")
            .filter(|(_, b)| !b.contains(" --> "))
            .ok_or_else(|| invalid("Malformed SRT timestamp line"))?;
        push_cue(
            doc,
            timestamp(start, "srt")?,
            timestamp(end, "srt")?,
            lines.next().unwrap_or("").into(),
            IndexMap::new(),
        )?;
    }
    Ok(())
}
fn push_cue(
    doc: &mut Document,
    start: i64,
    end: i64,
    text: String,
    fields: IndexMap<String, String>,
) -> Result<(), String> {
    if doc.cues.len() >= 100000 {
        return Err(invalid("Subtitle exceeds 100000 cues"));
    }
    doc.cues.push(Cue {
        id: format!("c{:06}", doc.cues.len() + 1),
        start,
        end,
        text,
        fields,
    });
    Ok(())
}
fn parse_ass(data: &str, doc: &mut Document) -> Result<(), String> {
    let mut seen = HashSet::new();
    for line in data.split('\n') {
        let header = ass_line_start(line).trim_end_matches([' ', '\t']);
        let folded = ass_key(header);
        if NATIVE.iter().any(|h| folded.starts_with(h)) && !NATIVE.contains(&folded.as_str()) {
            return Err(invalid(
                "ASS section header has a renderer-active suffix; repair its spelling explicitly",
            ));
        }
        let raw_assets = doc
            .sections
            .last()
            .is_some_and(|(name, _)| matches!(ass_key(name).as_str(), "[fonts]" | "[graphics]"));
        let known = NATIVE.contains(&folded.as_str())
            || matches!(
                folded.as_str(),
                "[graphics]" | "[aegisub project garbage]" | "[aegisub extradata]"
            );
        if header.starts_with('[') && header.ends_with(']') && (!raw_assets || known) {
            if !seen.insert(folded) {
                return Err(invalid("Repeated ASS sections are not supported"));
            }
            doc.sections.push((header.into(), Vec::new()));
        } else if let Some((_, lines)) = doc.sections.last_mut() {
            lines.push(line.into());
        } else if !header.is_empty() && !header.starts_with(';') {
            return Err(invalid("ASS content before the first section"));
        }
    }
    let mut style_position = None;
    let mut event_position = None;
    for (position, (section, lines)) in std::mem::take(&mut doc.sections).into_iter().enumerate() {
        let key = ass_key(&section);
        if key == "[v4+ styles]" || key == "[events]" {
            if key == "[events]" {
                event_position = Some(position);
            } else {
                style_position = Some(position);
            }
            read_rows(doc, &key, &lines)?;
        } else if key == "[v4 styles]" {
            return Err(invalid(
                "Legacy SSA requires explicit conversion to ASS first",
            ));
        } else if !matches!(key.as_str(), "[script info]" | "[fonts]")
            && lines.iter().any(|line| {
                let line = ass_line_start(line);
                [
                    "Format:",
                    "Style:",
                    "Dialogue:",
                    "; Script generated by FFmpeg/Lavc",
                ]
                .iter()
                .any(|p| line.starts_with(p))
                    || HEADERS
                        .iter()
                        .any(|h| line.strip_prefix(h).is_some_and(|s| s.starts_with(':')))
            })
        {
            return Err(invalid(
                "Renderer-active ASS row outside its supported section; repair a separate source copy",
            ));
        }
        doc.sections.push((section, lines));
    }
    if doc.event_fields.is_empty() || doc.style_fields.is_empty() {
        return Err(invalid("Missing ASS Events or V4+ Styles Format"));
    }
    if style_position > event_position {
        return Err(invalid(
            "ASS styles must precede events so style references resolve before rendering",
        ));
    }
    preserve_legacy(doc, event_position.unwrap_or(0));
    Ok(())
}
fn read_rows(doc: &mut Document, key: &str, lines: &[String]) -> Result<(), String> {
    let events = key == "[events]";
    let mut layout = Vec::new();
    for row in lines {
        if row.trim().is_empty() || row.trim_start().starts_with(';') {
            continue;
        }
        let (raw, value) = ass_line_start(row)
            .split_once(':')
            .ok_or_else(|| invalid("Malformed ASS section row"))?;
        let label = ass_key(raw);
        if let Some((_, canonical)) = [
            ("format", "Format"),
            ("dialogue", "Dialogue"),
            ("style", "Style"),
            ("comment", "Comment"),
        ]
        .iter()
        .find(|(l, _)| *l == label.trim_matches([' ', '\t']))
            && raw != *canonical
        {
            return Err(invalid(
                "ASS row labels require canonical spelling and no space before the colon",
            ));
        }
        match label.as_str() {
            "format" => {
                if !layout.is_empty() {
                    return Err(invalid("Multiple Format rows in an ASS section"));
                }
                layout = fields(value);
                if layout.iter().collect::<HashSet<_>>().len() != layout.len() {
                    return Err(invalid("Duplicate ASS Format fields"));
                }
                if events {
                    if !["start", "end", "text", "style"]
                        .iter()
                        .all(|k| layout.iter().any(|f| f == k))
                        || layout.last().map(String::as_str) != Some("text")
                    {
                        return Err(invalid(
                            "ASS Events requires Start, End, Style and final Text",
                        ));
                    }
                    doc.event_fields.clone_from(&layout);
                } else {
                    if !layout.iter().any(|f| f == "name") {
                        return Err(invalid("ASS Styles requires a Name field"));
                    }
                    doc.style_fields.clone_from(&layout);
                }
            }
            "comment" if events => doc.comments += 1,
            "dialogue" if events => read_row(doc, &layout, value, true)?,
            "style" if !events => read_row(doc, &layout, value, false)?,
            _ => {
                return Err(invalid(
                    "Unsupported ASS row in Events or Styles; inspect before conversion",
                ));
            }
        }
    }
    Ok(())
}
fn read_row(
    doc: &mut Document,
    layout: &[String],
    value: &str,
    events: bool,
) -> Result<(), String> {
    let value = value.trim_start_matches([' ', '\t']);
    if !events && value.is_empty() {
        return Err(invalid(
            "Empty ASS Style rows have renderer defaults; repair them explicitly",
        ));
    }
    if layout.is_empty() {
        return Err(invalid("ASS rows require a preceding Format"));
    }
    let parts: Vec<_> = value.splitn(layout.len(), ',').map(str::to_owned).collect();
    if parts.len() != layout.len() {
        return Err(invalid("ASS row does not match its Format"));
    }
    let row: IndexMap<_, _> = layout.iter().cloned().zip(parts.iter().cloned()).collect();
    let field = |key: &str| row.get(key).map_or("", String::as_str);
    if events {
        push_cue(
            doc,
            timestamp(field("start"), "ass")?,
            timestamp(field("end"), "ass")?,
            field("text").into(),
            row,
        )?;
    } else {
        let style = ass_style_name(field("name"), false).to_owned();
        if doc.styles.contains_key(&style) {
            return Err(invalid(
                "Duplicate ASS style name after renderer normalization",
            ));
        }
        doc.styles.insert(style, parts);
    }
    Ok(())
}
fn preserve_legacy(doc: &mut Document, event: usize) {
    if doc.styles.len() != 1
        || !doc
            .styles
            .keys()
            .next()
            .is_some_and(|s| s.starts_with("Default"))
    {
        return;
    }
    if let Some((_, lines)) = doc
        .sections
        .iter_mut()
        .take(event + 1)
        .find(|(section, _)| ass_key(section) == "[script info]")
    {
        let flags: HashSet<_> = lines
            .iter()
            .filter_map(|line| {
                let raw = ass_line_start(line);
                if raw.starts_with("; Script generated by FFmpeg/Lavc") {
                    Some("generator")
                } else {
                    raw.split_once(':')
                        .and_then(|(k, _)| HEADERS.contains(&k).then_some(k))
                }
            })
            .collect();
        if flags == HashSet::from(["ScriptType", "PlayResX", "PlayResY", "generator"]) {
            lines.push("ScaledBorderAndShadow: yes".into());
        }
    }
}
pub fn parse(data: &str, kind: &str) -> Result<Document, String> {
    if data.chars().take(16 * 1024 * 1024 + 1).count() > 16 * 1024 * 1024 {
        return Err(invalid("Subtitle text exceeds the supported parse budget"));
    }
    let bom = data.starts_with('\u{feff}');
    let line_ending = line_ending(data);
    let data = data
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    if data.contains(['\0', '\u{fffd}']) {
        return Err(invalid(
            "NUL or replacement characters in subtitles; check the input encoding",
        ));
    }
    let mut doc = Document {
        kind: kind.into(),
        bom,
        line_ending,
        ..Document::default()
    };
    match kind {
        "srt" => parse_srt(&data, &mut doc)?,
        "ass" => parse_ass(&data, &mut doc)?,
        _ => return Err(invalid("Supported formats are .ass and .srt")),
    }
    if doc.cues.is_empty() {
        return Err(invalid("No subtitle cues found"));
    }
    if let Some(cue) = doc.cues.iter().find(|cue| cue.end <= cue.start) {
        return Err(invalid(&format!(
            "Cue {} ends before or at its start",
            cue.id
        )));
    }
    Ok(doc)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentationOrder {
    Chronological,
    PreserveAssOverlaps,
}

pub fn presentation_order(cues: &[Cue], kind: &str) -> Result<Vec<usize>, String> {
    let policy = match kind {
        "ass" => PresentationOrder::PreserveAssOverlaps,
        "srt" => PresentationOrder::Chronological,
        _ => return Err(invalid("Cue ordering requires ASS or SRT format")),
    };
    let times = cues
        .iter()
        .map(|cue| effective_times(cue.start, cue.end, kind))
        .collect::<Result<Vec<_>, _>>()?;
    let mut order: Vec<_> = (0..times.len()).collect();
    order.sort_by_key(|&i| times[i].0);
    if policy == PresentationOrder::PreserveAssOverlaps {
        let mut begin = 0;
        let mut right = 0;
        for pos in 0..order.len() {
            let (start, end) = times[order[pos]];
            if start >= right {
                order[begin..pos].sort_unstable();
                begin = pos;
                right = end;
            } else {
                right = right.max(end);
            }
        }
        order[begin..].sort_unstable();
    }
    Ok(order)
}
pub fn style_references(cue: &Cue) -> Result<Vec<String>, String> {
    let mut styles = vec![
        ass_style_name(
            cue.fields.get("style").map_or("Default", String::as_str),
            true,
        )
        .to_owned(),
    ];
    styles.extend(
        ass_commands(&cue.text)?
            .into_iter()
            .filter_map(|(n, a)| (n == "r" && !a.is_empty()).then_some(a)),
    );
    styles.sort();
    styles.dedup();
    Ok(styles)
}
pub fn serialize(doc: &Document, cues: &[Cue], remove_fonts: bool) -> Result<String, String> {
    if cues.is_empty() {
        return Err(invalid("Refusing an episode with no retained cues"));
    }
    let order = if doc.chronological {
        let mut order = cues
            .iter()
            .enumerate()
            .map(|(i, c)| Ok((effective_times(c.start, c.end, &doc.kind)?.0, i)))
            .collect::<Result<Vec<_>, String>>()?;
        order.sort_unstable();
        order.into_iter().map(|(_, i)| i).collect()
    } else {
        presentation_order(cues, &doc.kind)?
    };
    if doc.kind == "srt" {
        let mut out = String::new();
        for (i, &index) in order.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let cue = &cues[index];
            let _ = writeln!(
                out,
                "{}\n{} --> {}\n{}",
                i + 1,
                timecode(cue.start, "srt")?,
                timecode(cue.end, "srt")?,
                cue.text
            );
        }
        return Ok(out);
    }
    let mut used = HashSet::new();
    for cue in cues {
        used.extend(style_references(cue)?);
    }
    if used.iter().any(|name| !doc.styles.contains_key(name)) {
        return Err(invalid("Subtitle references an undefined ASS style"));
    }
    let mut out = Vec::new();
    for (section, lines) in &doc.sections {
        let key = ass_key(section);
        if key == "[aegisub project garbage]" || (remove_fonts && key == "[fonts]") {
            continue;
        }
        out.push(section.clone());
        if key == "[events]" {
            out.push(format!("Format: {}", doc.event_fields.join(", ")));
            for &index in &order {
                let cue = &cues[index];
                let start = timecode(cue.start, "ass")?;
                let end = timecode(cue.end, "ass")?;
                if cue.text.contains(['\n', '\r']) {
                    return Err(invalid(
                        r"ASS cue text must use literal \N instead of a physical newline",
                    ));
                }
                let row = |text: &str| {
                    doc.event_fields
                        .iter()
                        .map(|k| match k.as_str() {
                            "start" => start.as_str(),
                            "end" => end.as_str(),
                            "text" => text,
                            _ => cue.fields.get(k).map_or("", String::as_str),
                        })
                        .collect::<Vec<_>>()
                        .join(",")
                };
                if let Some(references) = doc.source_comments.get(&cue.id) {
                    for r in references {
                        out.push(format!("Comment: {}", row(r)));
                    }
                }
                out.push(format!("Dialogue: {}", row(&cue.text)));
            }
        } else if key == "[v4+ styles]" {
            out.push(format!("Format: {}", doc.style_fields.join(", ")));
            out.extend(
                doc.styles
                    .iter()
                    .filter(|(name, _)| used.contains(*name))
                    .map(|(_, row)| format!("Style: {}", row.join(","))),
            );
        } else {
            out.extend(
                lines
                    .iter()
                    .filter(|line| {
                        !line.trim().is_empty()
                            && (matches!(key.as_str(), "[fonts]" | "[graphics]")
                                || !line.trim_start().starts_with(';'))
                    })
                    .cloned(),
            );
        }
        out.push(String::new());
    }
    Ok(out.join("\n"))
}
pub fn source_default_style_is_bold(doc: &Document) -> bool {
    if doc.kind != "ass" {
        return false;
    }
    let Some(index) = doc.style_fields.iter().position(|field| field == "bold") else {
        return false;
    };
    let Some(raw) = doc.styles.get("Default").and_then(|row| row.get(index)) else {
        return false;
    };
    if raw.trim().eq_ignore_ascii_case("true") {
        return true;
    }
    let value = raw.trim_start_matches([' ', '\t']);
    let (radix, digits) = ["&H", "&h", "0x", "0X"]
        .iter()
        .find_map(|prefix| value.strip_prefix(prefix))
        .map_or((10u32, value), |hex| (16, hex));
    // libass reads the integer prefix modulo 2^32. Its sign does not affect zero.
    let digits = digits.trim_start_matches([' ', '\t']);
    let digits = digits.strip_prefix(['+', '-']).unwrap_or(digits);
    // Its hexadecimal reader also accepts 0x after the sign and initial base marker.
    let digits = if radix == 16 {
        digits
            .strip_prefix("0x")
            .or_else(|| digits.strip_prefix("0X"))
            .unwrap_or(digits)
    } else {
        digits
    };
    digits
        .chars()
        .map_while(|c| c.to_digit(radix))
        .fold(0u32, |n, digit| n.wrapping_mul(radix).wrapping_add(digit))
        != 0
}

pub(crate) fn inherited_style_value(field: &str) -> String {
    // libass initializes native V4+ styles to zero, except for these three fields.
    match field {
        "scalex" | "scaley" => "100",
        "fontname" => "Arial",
        _ => "0",
    }
    .to_owned()
}

pub(crate) fn insert_generated_style(
    doc: &mut Document,
    name: &str,
    layout: &[String],
    row: &[String],
) {
    let missing: Vec<_> = layout
        .iter()
        .filter(|field| !doc.style_fields.contains(field))
        .cloned()
        .collect();
    if !missing.is_empty() {
        // Prepending keeps the source's last field last and preserves a custom format,
        // which enables libass's implicit ScaledBorderAndShadow setting.
        let inherited: Vec<_> = missing
            .iter()
            .map(|field| inherited_style_value(field))
            .collect();
        for existing in doc.styles.values_mut() {
            existing.splice(..0, inherited.iter().cloned());
        }
        doc.style_fields.splice(..0, missing);
    }
    let generated = doc.styles.entry(name.to_owned()).or_default();
    generated.resize_with(doc.style_fields.len(), String::new);
    for (field, value) in layout.iter().zip(row) {
        if let Some(index) = doc.style_fields.iter().position(|f| f == field) {
            generated[index].clone_from(value);
        }
    }
}

// Only reconcile widened rows from formerly identical source layouts. Other donor
// schemas must still be adapted explicitly so unrelated presentation changes stay visible.
pub(crate) fn donor_style_prefix<'a>(base: &'a Document, donor: &Document) -> Option<&'a [String]> {
    let prefix = base
        .style_fields
        .strip_suffix(donor.style_fields.as_slice())?;
    if prefix.is_empty() {
        return Some(prefix);
    }
    let original = base
        .sections
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("[V4+ Styles]"))?
        .1
        .iter()
        .find_map(|line| ass_line_start(line).strip_prefix("Format:"))?;
    (fields(original) == donor.style_fields).then_some(prefix)
}

pub fn ass_template(
    style_line: Option<&str>,
    title: &str,
    x: i64,
    y: i64,
) -> Result<Document, String> {
    let style_fields = fields(STYLE_FIELDS);
    let event_fields = fields(ASS_FIELDS);
    let (name, style) = generated_style(style_line)?;
    if x <= 0 || y <= 0 {
        return Err(invalid("ASS script resolution requires positive integers"));
    }
    let mut info = vec![
        "ScriptType: v4.00+".into(),
        format!("PlayResX: {x}"),
        format!("PlayResY: {y}"),
        "ScaledBorderAndShadow: yes".into(),
        "WrapStyle: 0".into(),
    ];
    if !title.is_empty() {
        if title.contains(['\r', '\n', '\0']) {
            return Err(invalid("ASS title must fit on one line"));
        }
        info.insert(0, format!("Title: {title}"));
    }
    let styles = IndexMap::from([(name, style)]);
    Ok(Document {
        kind: "ass".into(),
        sections: vec![
            ("[Script Info]".into(), info),
            ("[V4+ Styles]".into(), Vec::new()),
            ("[Events]".into(), Vec::new()),
        ],
        event_fields,
        style_fields,
        styles,
        ..Document::default()
    })
}

pub(crate) fn generated_style(style_line: Option<&str>) -> Result<(String, Vec<String>), String> {
    let mut style: Vec<_> = style_line
        .map_or(DEFAULT_STYLE, |s| {
            s.strip_prefix("Style:")
                .unwrap_or(s)
                .trim_start_matches([' ', '\t'])
        })
        .split(',')
        .map(str::to_owned)
        .collect();
    if style.len() != fields(STYLE_FIELDS).len()
        || style[0].trim().is_empty()
        || style.iter().any(|v| v.contains(['\r', '\n', '\0']))
    {
        return Err(invalid("ASS style must be a single V4+ Style row"));
    }
    // Generated definitions must use the same name the ASS renderer resolves
    // from their event references; meaningful non-ASCII spaces remain literal.
    let name = ass_style_name(ass_style_name(&style[0], false), true).to_owned();
    style[0] = name.clone();
    Ok((name, style))
}
pub fn ass_commands(text: &str) -> Result<Vec<(String, String)>, String> {
    markup::ass_commands(text)
}
pub fn srt_tags(text: &str) -> Result<Vec<String>, String> {
    Ok(markup::pieces(text, "srt")?
        .into_iter()
        .filter_map(|(k, v)| (k == "tag").then_some(v.to_owned()))
        .collect())
}
pub fn srt_to_ass(cue: Cue) -> Result<Cue, String> {
    super::conversion::srt_to_ass(cue)
}
