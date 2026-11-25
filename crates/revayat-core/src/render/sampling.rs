use crate::subtitle_formats as formats;
use crate::subtitle_formats::document::{Cue, Document, ass_commands, srt_tags};
use crate::workflow::contract::{item, items, iterable, python_type, truth, value_error};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub const SAMPLER_VERSION: i64 = 12;
pub const MAX_RENDER_FRAMES: usize = 5000;

pub fn animated(cue: &Cue, commands: &[(String, String)]) -> bool {
    cue.fields.get("effect").is_some_and(|effect| {
        ["Banner;", "Scroll up;", "Scroll down;"]
            .iter()
            .any(|prefix| effect.starts_with(prefix))
    }) || commands.iter().any(|(name, _)| {
        matches!(
            name.as_str(),
            "t" | "k" | "K" | "kf" | "ko" | "kt" | "move" | "fad" | "fade"
        )
    })
}

pub fn changed_indices(episode: &Value) -> Result<Vec<i64>, String> {
    let provenance = item(episode, "provenance")?;
    let mut changed = Vec::new();
    for row in iterable(provenance)? {
        if !row.is_object() {
            return Err(format!(
                "AttributeError: '{}' object has no attribute 'get'",
                python_type(row)
            ));
        }
        if truth(&row["structure_changed"]) {
            changed.push(
                item(row, "emitted_index")?
                    .as_i64()
                    .ok_or_else(|| "TypeError: emitted_index is not an integer".to_owned())?,
            );
        }
    }
    Ok(changed)
}

pub fn samples(doc: &Document, all_cues: bool, changed: &[i64]) -> Result<Vec<Value>, String> {
    let cues = &doc.cues;
    if cues.is_empty() {
        return Err(value_error("Render sampling requires at least one cue"));
    }
    if doc.format().is_none() {
        return Err(value_error(
            "Render sampling requires an ASS or SRT document",
        ));
    }
    let (mut earliest, mut latest, mut ending) = (0, 0, 0);
    let mut styles = HashSet::new();
    let mut signatures = HashSet::new();
    let mut chosen = BTreeSet::from([0, cues.len() - 1]);
    let mut animations = Vec::with_capacity(cues.len());
    let ass = doc.kind == "ass";
    for (index, cue) in cues.iter().enumerate() {
        // Public callers can construct Document directly, bypassing the parser.
        formats::document::effective_times(cue.start, cue.end, &doc.kind)?;
        if cue.start < cues[earliest].start {
            earliest = index;
        }
        if cue.start > cues[latest].start {
            latest = index;
        }
        if cue.end > cues[ending].end {
            ending = index;
        }
        let commands = if ass {
            ass_commands(&cue.text)?
        } else {
            srt_tags(&cue.text)?
                .into_iter()
                .map(|tag| ("srt-tag".into(), caseless::default_case_fold_str(&tag)))
                .collect()
        };
        let references = if ass {
            crate::subtitle_formats::document::style_references(cue)?
        } else {
            vec!["Default".into()]
        };
        let new_style = references.iter().any(|style| !styles.contains(style));
        let animated = animated(cue, &commands);
        animations.push(animated);
        let field = |key, default| cue.fields.get(key).map_or(default, String::as_str);
        let signature = (
            field("style", "Default"),
            commands,
            ["layer", "marginl", "marginr", "marginv", "effect"].map(|key| field(key, "")),
        );
        let prose = formats::visible(cue.text.clone(), doc.kind.clone())?;
        let risky = crate::markup::has_rtl(prose.clone())?
            && (crate::markup::has_ltr(prose.clone())?
                || prose.contains(['"', '\'', '«', '»', '“', '”', '(', ')', '<', '>', '[', ']']));
        let new_signature = signatures.insert(signature);
        if all_cues
            || animated
            || risky
            || formats::has_drawing(cue.text.clone(), doc.kind.clone())?
            || new_style
            || new_signature
            || prose.contains('\n')
        {
            chosen.insert(index);
        }
        styles.extend(references);
    }
    chosen.extend([earliest, latest]);
    if ass {
        chosen.insert(ending);
    }
    for &index in changed {
        let index = index
            .checked_sub(1)
            .and_then(|n| usize::try_from(n).ok())
            .filter(|&i| i < cues.len())
            .ok_or_else(|| value_error("Changed-structure cue index is outside the episode"))?;
        chosen.insert(index);
    }
    let mut times = BTreeMap::new();
    for index in chosen {
        let cue = &cues[index];
        let fractions: &[f64] = if animations[index] {
            &[0.1, 0.5, 0.9]
        } else {
            &[0.5]
        };
        let span = cue
            .end
            .checked_sub(cue.start)
            .ok_or_else(|| value_error("Cue timing span exceeds the supported range"))?;
        for &fraction in fractions {
            let seconds = (cue.start as f64 + span as f64 * fraction) / 1000.0;
            let rounded = format!("{seconds:.4}")
                .parse::<f64>()
                .map_err(value_error)?;
            let units = (rounded * 10_000.0).round() as i64;
            times
                .entry(units)
                .or_insert_with(BTreeSet::new)
                .insert(index + 1);
            if times.len() > MAX_RENDER_FRAMES {
                return Err(value_error(
                    "Render sampling exceeds the 5000-frame episode budget",
                ));
            }
        }
    }
    Ok(times
        .into_iter()
        .map(|(time, indices)| json!({"time_seconds": time as f64 / 10_000.0, "cue_indices": indices}))
        .collect())
}

pub fn requested_fonts(doc: &Document) -> Result<Vec<String>, String> {
    let mut names = BTreeSet::new();
    if let Some(index) = doc
        .style_fields
        .iter()
        .position(|field| field == "fontname")
    {
        for row in doc.styles.values() {
            let name = row
                .get(index)
                .ok_or_else(|| "IndexError: style row is shorter than the font field".to_owned())?
                .trim();
            if !name.is_empty() {
                names.insert(name.to_owned());
            }
        }
    }
    for cue in &doc.cues {
        if doc.kind == "ass" {
            for (name, arg) in ass_commands(&cue.text)? {
                let arg = arg.trim();
                if name == "fn" && !arg.is_empty() {
                    names.insert(arg.to_owned());
                }
            }
        } else {
            for tag in srt_tags(&cue.text)? {
                let fonts = crate::markup::srt_font_names(tag)?;
                names.extend(
                    items(&fonts["$set"])
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned),
                );
            }
        }
    }
    Ok(names.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subtitle_formats::document::parse;

    #[test]
    fn shared_sample_tracks_both_cues_and_rejects_foreign_index() -> Result<(), String> {
        let doc = parse(
            "1\n00:00:01,000 --> 00:00:02,000\nسلام\n\n2\n00:00:01,200 --> 00:00:01,800\nSecond cue\n",
            "srt",
        )?;
        assert_eq!(
            samples(&doc, true, &[])?,
            [json!({"time_seconds": 1.5, "cue_indices": [1, 2]})]
        );
        assert!(samples(&doc, false, &[3]).is_err());
        Ok(())
    }
}
