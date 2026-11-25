use super::*;
use std::collections::HashSet;
use std::net::Ipv6Addr;

fn digits(s: &str, prefix: char, count: usize) -> bool {
    s.strip_prefix(prefix)
        .is_some_and(|s| s.len() == count && s.bytes().all(|b| b.is_ascii_digit()))
}
fn source(s: &str) -> bool {
    digits(s, 's', 4)
}
fn cue(s: &str) -> bool {
    digits(s, 'c', 6)
}
fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn language(s: &str) -> bool {
    let mut parts = s.split('-');
    parts
        .next()
        .is_some_and(|s| (2..=8).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphabetic()))
        && parts.all(|s| (1..=8).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric()))
}
fn url(s: &str) -> bool {
    if s.chars().any(|c| ws(c) || c < ' ') {
        return false;
    }
    let Some((scheme, tail)) = s.split_once("://") else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return false;
    }
    let authority = tail.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return false;
    }
    let valid_port = |port: &str| {
        port.is_empty()
            || (port.bytes().all(|b| b.is_ascii_digit())
                && port.trim_start_matches('0').parse::<u16>().is_ok()
                || port.bytes().all(|b| b == b'0'))
    };
    if let Some(tail) = authority.strip_prefix('[') {
        let Some((host, rest)) = tail.split_once(']') else {
            return false;
        };
        let (address, zone) = host
            .split_once('%')
            .map_or((host, None), |(a, z)| (a, Some(z)));
        return address.parse::<Ipv6Addr>().is_ok()
            && zone.is_none_or(|z| !z.is_empty() && !z.contains('%'))
            && (rest.is_empty() || rest.strip_prefix(':').is_some_and(valid_port));
    }
    let host = match authority.split_once(':') {
        Some((h, p)) if valid_port(p) => h,
        Some(_) => return false,
        None => authority,
    };
    let Ok(host) = idna::domain_to_ascii(host) else {
        return false;
    };
    host.len() <= 253
        && host.trim_end_matches('.').split('.').all(|p| {
            !p.is_empty()
                && p.len() <= 63
                && !p.starts_with('-')
                && !p.ends_with('-')
                && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

pub(super) fn glossary(value: &Value, loc: &str) -> Result<(), String> {
    object(value, loc)?;
    text(&value["series"], &format!("{loc}.series"), false)?;
    if let Some(v) = value.get("terms_reviewed") {
        flag(v, &format!("{loc}.terms_reviewed"))?;
    }
    for (i, item) in optional(value.get("research"), &format!("{loc}.research"), 10000)?
        .iter()
        .enumerate()
    {
        let loc = format!("{loc}.research[{i}]");
        object(item, &loc)?;
        let field = format!("{loc}.url");
        if !url(text(&item["url"], &field, false)?) {
            return Err(invalid(
                &field,
                "expected an HTTP(S) URL with a hostname and no credentials",
            ));
        }
        text(&item["note"], &format!("{loc}.note"), false)?;
    }
    for (i, item) in optional(value.get("terms"), &format!("{loc}.terms"), 100000)?
        .iter()
        .enumerate()
    {
        let loc = format!("{loc}.terms[{i}]");
        object(item, &loc)?;
        for k in ["source", "target"] {
            text(&item[k], &format!("{loc}.{k}"), false)?;
        }
        flag(&item["locked"], &format!("{loc}.locked"))?;
    }
    Ok(())
}

pub(super) fn project(value: &Value) -> Result<(), String> {
    object(value, "project.json")?;
    let version = value["version"].as_i64();
    if !matches!(version, Some(1 | 2)) {
        return Err(invalid(
            "project.json.version",
            "unsupported project schema",
        ));
    }
    text(&value["series"], "project.json.series", false)?;
    bounded(&value["season"], "project.json.season", 1, 99)?;
    pattern(
        &value["target_language"],
        language,
        "project.json.target_language",
    )?;
    if !matches!(value["font_policy"].as_str(), Some("keep" | "remove")) {
        return Err(invalid(
            "project.json.font_policy",
            "expected keep or remove",
        ));
    }
    if let Some(output) = value.get("output_format")
        && !matches!(output.as_str(), Some("ass" | "srt"))
    {
        return Err(invalid("project.json.output_format", "expected ass or srt"));
    }
    for (i, item) in items(&value["sources"], "project.json.sources", 1000)?
        .iter()
        .enumerate()
    {
        let loc = format!("project.json.sources[{i}]");
        object(item, &loc)?;
        pattern(&item["id"], source, &format!("{loc}.id"))?;
        pattern(&item["sha256"], hash, &format!("{loc}.sha256"))?;
        if !matches!(item["kind"].as_str(), Some("ass" | "srt")) {
            return Err(invalid(&format!("{loc}.kind"), "expected ass or srt"));
        }
        for k in ["name", "file", "encoding"] {
            text(&item[k], &format!("{loc}.{k}"), false)?;
        }
        text_encoding(item["encoding"].clone(), format!("{loc}.encoding"))?;
        bounded(&item["cues"], &format!("{loc}.cues"), 1, 100000)?;
        let origin = &item["origin"];
        if version == Some(2) || item.get("origin").is_some() {
            let loc = format!("{loc}.origin");
            object(origin, &loc)?;
            for k in ["root_id", "root_label", "relative_path"] {
                text(&origin[k], &format!("{loc}.{k}"), false)?;
            }
            if !origin["member"].is_null() {
                text(&origin["member"], &format!("{loc}.member"), false)?;
                bounded(
                    &origin["member_index"],
                    &format!("{loc}.member_index"),
                    1,
                    10000,
                )?;
            }
        }
    }
    for (i, item) in items(&value["episodes"], "project.json.episodes", 1000)?
        .iter()
        .enumerate()
    {
        let loc = format!("project.json.episodes[{i}]");
        object(item, &loc)?;
        for k in ["id", "base", "comparison"] {
            text(&item[k], &format!("{loc}.{k}"), false)?;
        }
        for (j, id) in optional(item.get("alternates"), &format!("{loc}.alternates"), 1000)?
            .iter()
            .enumerate()
        {
            pattern(id, source, &format!("{loc}.alternates[{j}]"))?;
        }
    }
    Ok(())
}

pub(super) fn worksheet(value: &Value, loc: &str) -> Result<(), String> {
    for (i, row) in items(value, loc, 100000)?.iter().enumerate() {
        let loc = format!("{loc}[{i}]");
        object(row, &loc)?;
        pattern(&row["id"], cue, &format!("{loc}.id"))?;
        text(&row["source_text"], &format!("{loc}.source_text"), true)?;
        flag(&row["reviewed"], &format!("{loc}.reviewed"))?;
        for k in ["start_ms", "end_ms"] {
            bounded(&row[k], &format!("{loc}.{k}"), 0, MAX_TIME_MS)?;
        }
        if !matches!(
            row["action"].as_str(),
            Some("edit" | "preserve" | "credit" | "empty" | "alternate")
        ) {
            return Err(invalid(
                &format!("{loc}.action"),
                "expected a valid review action",
            ));
        }
        if !row["text"].is_null() {
            text(&row["text"], &format!("{loc}.text"), true)?;
        }
        for k in ["note", "timing_note", "structure_note", "direction_note"] {
            if let Some(v) = row.get(k) {
                text(v, &format!("{loc}.{k}"), true)?;
            }
        }
        if let Some(v) = row.get("direction")
            && !matches!(v.as_str(), Some("auto" | "ltr" | "rtl"))
        {
            return Err(invalid(
                &format!("{loc}.direction"),
                "expected auto, ltr or rtl",
            ));
        }
        if let Some(v) = row.get("preserve_empty_lines") {
            flag(v, &format!("{loc}.preserve_empty_lines"))?;
        }
        for (j, v) in optional(row.get("links"), &format!("{loc}.links"), 1000)?
            .iter()
            .enumerate()
        {
            pattern(
                v,
                |s| s.split_once(':').is_some_and(|(a, b)| source(a) && cue(b)),
                &format!("{loc}.links[{j}]"),
            )?;
        }
    }
    Ok(())
}

pub(super) fn frame(v: &Value, loc: &str, review: bool) -> Result<(), String> {
    object(v, loc)?;
    text(&v["file"], &format!("{loc}.file"), false)?;
    pattern(&v["sha256"], hash, &format!("{loc}.sha256"))?;
    finite(v["time_seconds"].as_f64(), &format!("{loc}.time_seconds"))?;
    for k in ["width", "height"] {
        bounded(&v[k], &format!("{loc}.{k}"), 1, 8192)?;
    }
    let loc_indices = format!("{loc}.cue_indices");
    let indices = items(&v["cue_indices"], &loc_indices, 100000)?;
    if indices.is_empty() {
        return Err(invalid(
            &loc_indices,
            "expected associated emitted cue indices",
        ));
    }
    let mut unique = HashSet::with_capacity(indices.len());
    for i in indices {
        unique.insert(bounded(i, &loc_indices, 1, 100000)?);
    }
    if unique.len() != indices.len() {
        return Err(invalid(&loc_indices, "duplicate cue index"));
    }
    if review {
        flag(&v["reviewed"], &format!("{loc}.reviewed"))?;
        text(&v["note"], &format!("{loc}.note"), true)?;
    }
    Ok(())
}
pub(super) fn render_record(v: &Value, loc: &str) -> Result<(), String> {
    object(v, loc)?;
    if v["version"].as_i64() != Some(2) {
        return Err(invalid(
            &format!("{loc}.version"),
            "legacy or unsupported render schema; rerender this edition",
        ));
    }
    for k in ["identity", "subtitle_sha256", "receipt_sha256"] {
        pattern(&v[k], hash, &format!("{loc}.{k}"))?;
    }
    for k in ["episode", "receipt_file"] {
        text(&v[k], &format!("{loc}.{k}"), false)?;
    }
    flag(&v["all_cues"], &format!("{loc}.all_cues"))?;
    if !matches!(v["background"].as_str(), Some("video" | "neutral")) {
        return Err(invalid(
            &format!("{loc}.background"),
            "expected video or neutral",
        ));
    }
    for (i, item) in items(&v["frames"], &format!("{loc}.frames"), 5000)?
        .iter()
        .enumerate()
    {
        frame(item, &format!("{loc}.frames[{i}]"), true)?;
    }
    Ok(())
}
pub(super) fn fingerprint(v: &Value, loc: &str, max: i64) -> Result<(), String> {
    object(v, loc)?;
    let field = format!("{loc}.name");
    if text(&v["name"], &field, false)?.contains(['/', '\\']) {
        return Err(invalid(&field, "expected a basename, not a private path"));
    }
    bounded(&v["bytes"], &format!("{loc}.bytes"), 0, max)?;
    pattern(&v["sha256"], hash, &format!("{loc}.sha256"))?;
    Ok(())
}
