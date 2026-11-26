use super::contract::{items, string, value_error};
use crate::markup::{clean_empty_lines, paragraph_direction, remap_resets, rtl, uncomment};
use crate::subtitle_formats as formats;
use crate::subtitle_formats::document::{Cue, Document};
use crate::validation;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashSet};

pub fn verify_glossary(terms: Value, series: &str) -> Result<(), String> {
    let terms = validation::glossary(terms, "glossary.json".into())?;
    if terms["series"] != series || terms["terms_reviewed"] != true {
        return Err(value_error("Review the series glossary before building"));
    }
    if items(&terms["research"]).is_empty() {
        return Err(value_error(
            "Record actual anime research URLs and what they establish",
        ));
    }
    let mut seen = HashSet::new();
    for term in items(&terms["terms"]) {
        if string(&term["source"]).is_empty()
            || string(&term["target"]).is_empty()
            || term["locked"] != true
        {
            return Err(value_error(
                "Each glossary term needs source, target, and locked=true",
            ));
        }
        if !seen.insert(string(&term["source"])) {
            return Err(value_error("Duplicate glossary source term"));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CueDecision<'a> {
    Keep,
    Alternate { target_id: &'a str },
    Drop { reason: &'a str },
}

pub fn reviewed_cues(
    doc: &Document,
    sheet: &[Value],
    language: &str,
) -> Result<(Vec<Cue>, Vec<Value>), String> {
    let mut kept = Vec::new();
    let mut decisions = Vec::new();
    if !sheet
        .iter()
        .map(|row| row["id"].as_str())
        .eq(doc.cues.iter().map(|cue| Some(cue.id.as_str())))
    {
        return Err(value_error(
            "Worksheet must contain every original cue ID exactly once, in original order",
        ));
    }
    let kind = &doc.kind;
    for (cue, row) in doc.cues.iter().zip(sheet) {
        let id = &cue.id;
        if row["source_text"] != cue.text || row["reviewed"] != true {
            return Err(value_error(format!(
                "Cue {id} has changed source text or has not been reviewed"
            )));
        }
        let action = string(&row["action"]);
        let note = string(&row["note"]);
        let links = items(&row["links"]);
        let visible = |text: &str| -> Result<bool, String> {
            Ok(!formats::visible(text.into(), kind.clone())?.is_empty()
                || formats::has_drawing(text.into(), kind.clone())?)
        };
        let decision = match action {
            "credit" | "alternate" if note.trim().is_empty() => {
                return Err(value_error(format!(
                    "Cue {id} needs a reason for exclusion"
                )));
            }
            "empty" if visible(&cue.text)? => {
                return Err(value_error("A nonempty cue cannot be discarded as empty"));
            }
            "alternate" if links.is_empty() => {
                return Err(value_error(
                    "An alternate cue must link to the retained cue covering its content",
                ));
            }
            "edit" | "preserve" => CueDecision::Keep,
            "credit" | "empty" => CueDecision::Drop { reason: note },
            "alternate" => CueDecision::Alternate {
                target_id: string(&links[0]),
            },
            _ => return Err(value_error(format!("Cue {id} needs a valid review action"))),
        };
        decisions.push(json!({"id": id, "action": action, "links": links, "note": note}));
        if decision != CueDecision::Keep {
            continue;
        }
        let text = if action == "preserve" {
            &cue.text
        } else {
            string(&row["text"])
        };
        let text = if kind == "srt" {
            text.replace("\r\n", "\n").replace('\r', "\n")
        } else {
            text.to_owned()
        };
        if !visible(&text)? {
            return Err(value_error(format!(
                "Cue {id} requires nonempty reviewed text"
            )));
        }
        if kind == "ass" && text.contains(['\n', '\r']) {
            return Err(value_error(format!(
                r"Cue {id}: ASS text must use literal \N, not physical newlines"
            )));
        }
        let structure_reason = !string(&row["structure_note"]).trim().is_empty();
        if formats::structure(text.clone(), kind.clone())?
            != formats::structure(cue.text.clone(), kind.clone())?
            && !structure_reason
        {
            return Err(value_error(
                "Changed tags/drawings require a reason and visual review",
            ));
        }
        let timing = row["start_ms"]
            .as_i64()
            .zip(row["end_ms"].as_i64())
            .filter(|&(start, end)| start >= 0 && start < end);
        let (start, end) = timing.ok_or_else(|| {
            value_error("Cue timing must be nonnegative integer milliseconds with end after start")
        })?;
        if (start, end) != (cue.start, cue.end) && string(&row["timing_note"]).trim().is_empty() {
            return Err(value_error("Timing changes require an alignment reason"));
        }
        let text = uncomment(text, kind.clone())?;
        let preserve = row["preserve_empty_lines"] == true;
        if preserve && !structure_reason {
            return Err(value_error(format!(
                "Cue {id}: preserving empty display lines needs a structure_note"
            )));
        }
        if preserve
            && kind == "srt"
            && text
                .split('\n')
                .skip(1)
                .take(
                    text.bytes()
                        .filter(|&b| b == b'\n')
                        .count()
                        .saturating_sub(1),
                )
                .any(|line| line.trim_matches([' ', '\t']).is_empty())
        {
            return Err(value_error(format!(
                "Cue {id}: SRT cannot preserve a physical cue-block separator; choose a representable layout"
            )));
        }
        let text = clean_empty_lines(text, kind.clone(), preserve)?;
        let direction = row["direction"].as_str().unwrap_or("auto");
        let force = direction != "auto";
        let direction = if force {
            if string(&row["direction_note"]).trim().is_empty() {
                return Err(value_error(format!(
                    "Cue {id}: direction override requires direction_note"
                )));
            }
            direction.into()
        } else {
            paragraph_direction(language.into())?
        };
        kept.push(Cue {
            id: id.clone(),
            start,
            end,
            text: rtl(text, kind.clone(), direction, force)?,
            fields: cue.fields.clone(),
        });
    }
    Ok((kept, decisions))
}

const TRACK_FIELDS: [&str; 11] = [
    "PlayResX",
    "PlayResY",
    "LayoutResX",
    "LayoutResY",
    "WrapStyle",
    "ScaledBorderAndShadow",
    "Kerning",
    "YCbCr Matrix",
    "Language",
    "Collisions",
    "Timer",
];

fn script_info(doc: &Document) -> Result<BTreeMap<String, &str>, String> {
    let mut result = BTreeMap::new();
    for (name, lines) in &doc.sections {
        if !name.eq_ignore_ascii_case("[script info]") {
            continue;
        }
        for line in lines {
            let Some((label, value)) = line.trim_start_matches([' ', '\t']).split_once(':') else {
                continue;
            };
            let key = label.trim_matches([' ', '\t']).to_ascii_lowercase();
            let Some(canonical) = TRACK_FIELDS.iter().find(|s| s.eq_ignore_ascii_case(&key)) else {
                continue;
            };
            if label != *canonical {
                return Err(value_error(format!(
                    "ASS track settings require canonical header spelling ({key})"
                )));
            }
            let stripped = value.trim_start_matches([' ', '\t']);
            if matches!(
                key.as_str(),
                "playresx" | "playresy" | "layoutresx" | "layoutresy" | "wrapstyle"
            ) && stripped != value
                && ["&h", "0x"]
                    .iter()
                    .any(|prefix| stripped.to_ascii_lowercase().starts_with(prefix))
            {
                return Err(value_error(format!(
                    "ASS hexadecimal track settings with leading padding require explicit repair ({key})"
                )));
            }
            result.insert(key, stripped.trim_end_matches([' ', '\t']));
        }
    }
    Ok(result)
}

pub fn donor(
    base: &mut Document,
    donor: &Document,
    mut cues: Vec<Cue>,
    source: &str,
    keep_fonts: bool,
) -> Result<Vec<Cue>, String> {
    use crate::subtitle_formats::document::{
        ass_template, donor_style_prefix, inherited_style_value, insert_generated_style, srt_to_ass,
    };
    if cues.is_empty() || base.kind == "srt" && donor.kind == "srt" {
        return Ok(cues);
    }
    if base.kind == "srt" {
        return Err(value_error(
            "An ASS candidate must be the base to preserve its presentation",
        ));
    }
    let mut candidate = base.clone();
    if donor.kind == "srt" {
        if !candidate.styles.contains_key("Default") {
            let template = ass_template(None, "", 1920, 1080)?;
            let default = template
                .styles
                .get("Default")
                .ok_or_else(|| value_error("Default style template is missing"))?;
            insert_generated_style(&mut candidate, "Default", &template.style_fields, default);
        }
        let result = cues.into_iter().map(srt_to_ass).collect::<Result<_, _>>()?;
        base.styles = candidate.styles;
        base.style_fields = candidate.style_fields;
        return Ok(result);
    }
    let Some(prefix) = donor_style_prefix(&candidate, donor)
        .filter(|_| candidate.event_fields == donor.event_fields)
    else {
        return Err(value_error(
            "ASS candidate formats differ; adapt them explicitly before merging",
        ));
    };
    let inherited: Vec<_> = prefix
        .iter()
        .map(|field| inherited_style_value(field))
        .collect();
    let a = script_info(&candidate)?;
    let b = script_info(donor)?;
    let differences: Vec<_> = TRACK_FIELDS
        .iter()
        .map(|field| field.to_ascii_lowercase())
        .filter(|field| a.get(field) != b.get(field))
        .collect();
    if !differences.is_empty() {
        return Err(value_error(format!(
            "ASS candidates use different canvas/wrapping or track settings ({}); align before merging",
            differences.join(", ")
        )));
    }
    let mut mapping = Map::new();
    for (name, values) in &donor.styles {
        let mapped = format!("{source}__{name}");
        if candidate.styles.contains_key(&mapped) {
            return Err(value_error("Donor style namespace collision"));
        }
        let index = candidate
            .style_fields
            .iter()
            .position(|key| key == "name")
            .ok_or_else(|| value_error("Donor style format lacks a name"))?;
        let mut values: Vec<_> = inherited.iter().chain(values).cloned().collect();
        *values
            .get_mut(index)
            .ok_or_else(|| value_error("Donor style format lacks a name"))? = mapped.clone();
        mapping.insert(name.clone(), json!(mapped));
        candidate.styles.insert(mapped, values);
    }
    if keep_fonts {
        let donor_fonts = fonts(donor);
        let base_fonts = fonts(&candidate);
        if !base_fonts.is_empty() && !donor_fonts.is_empty() && base_fonts != donor_fonts {
            return Err(value_error(
                "Different embedded font sets: reconcile fonts before merging ASS donor events",
            ));
        }
        if let (true, Some(lines)) = (base_fonts.is_empty(), donor_fonts.first()) {
            candidate
                .sections
                .push(("[Fonts]".into(), (*lines).clone()));
        }
    }
    let mapping = Value::Object(mapping);
    for cue in &mut cues {
        let style = cue
            .fields
            .get_mut("style")
            .ok_or_else(|| value_error("Donor references an undefined style"))?;
        let key = formats::ass_style_name(style.clone(), true)?;
        *style = mapping[&key]
            .as_str()
            .ok_or_else(|| value_error("Donor references an undefined style"))?
            .into();
        cue.text = remap_resets(std::mem::take(&mut cue.text), mapping.clone())?;
    }
    base.styles = candidate.styles;
    base.sections = candidate.sections;
    Ok(cues)
}

fn fonts(doc: &Document) -> Vec<&Vec<String>> {
    doc.sections
        .iter()
        .filter_map(|(name, lines)| name.eq_ignore_ascii_case("[fonts]").then_some(lines))
        .collect()
}
