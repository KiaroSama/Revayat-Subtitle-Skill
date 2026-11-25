//! Validate revision-bound editorial proposals without importing or approving them.

#![forbid(unsafe_code)]

use crate::runtime::files::{local_path, read_json, read_limited};
use crate::status::{WorkspaceState, bounded_report, read_workspace};
use crate::validation;
use crate::workflow::contract::{
    attr_path, io_error, items, iterable, linked, python_type, string, value_error, workspace_path,
};
use crate::workflow::review::reviewed_cues;
use crate::workflow::workspace::{Workspace, digest, pending, source_keys};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use subtitle_document::single_cue;

const MAX_FILE: u64 = 16 * 1024 * 1024;
const ASSIGNMENT: &[&str] = &[
    "version",
    "id",
    "source",
    "episode",
    "source_sha256",
    "glossary_sha256",
    "assigned_ids",
    "context_ids",
];
const RESULT: &[&str] = &[
    "version",
    "assignment_id",
    "source_sha256",
    "glossary_sha256",
    "rows",
    "status",
    "activity_log",
];
const ROW: &[&str] = &[
    "id",
    "source_text",
    "start_ms",
    "end_ms",
    "action",
    "text",
    "reviewed",
    "links",
    "note",
    "timing_note",
    "structure_note",
    "direction",
    "direction_note",
    "preserve_empty_lines",
];

pub fn read_envelope(path: &Path, fields: &[&str], required: &[&str]) -> Result<Value, String> {
    let path = std::path::absolute(path).map_err(io_error)?;
    for entry in path.ancestors() {
        match fs::symlink_metadata(entry) {
            Ok(metadata) if linked(&metadata) => {
                return Err(value_error("Linked handoff artifacts are not accepted"));
            }
            Ok(_) => (),
            Err(e) if e.kind() == ErrorKind::NotFound => (),
            Err(e) => return Err(io_error(e)),
        }
    }
    let value = validation::obj(read_json(&path, MAX_FILE)?, "handoff envelope".into())?;
    if required.iter().any(|key| value.get(key).is_none())
        || value
            .as_object()
            .into_iter()
            .flat_map(|map| map.keys())
            .any(|key| !fields.contains(&key.as_str()))
    {
        return Err(value_error(
            "Handoff envelope has missing or unsupported fields",
        ));
    }
    if value["version"].as_i64() != Some(1) {
        return Err(value_error("Unsupported handoff envelope version"));
    }
    Ok(value)
}

fn valid_id(value: &Value, prefix: &str, digits: usize) -> bool {
    value
        .as_str()
        .and_then(|s| s.strip_prefix(prefix))
        .is_some_and(|s| s.len() == digits && s.bytes().all(|b| b.is_ascii_digit()))
}

fn ids<'a>(value: &'a Value, at: &str) -> Result<BTreeSet<&'a str>, String> {
    if !value.is_array() || items(value).len() > 100_000 {
        return Err(value_error(format!(
            "{at}: expected an array with at most 100000 entries"
        )));
    }
    let rows = items(value);
    if rows.iter().any(|id| !valid_id(id, "c", 6)) {
        return Err(value_error(format!("{at}: invalid identifier or digest")));
    }
    let result: BTreeSet<_> = rows.iter().map(string).collect();
    if result.len() != rows.len() {
        return Err(value_error("Handoff cue IDs must be unique"));
    }
    Ok(result)
}

fn assignment_id<'a>(value: &'a Value, at: &str) -> Result<&'a str, String> {
    value
        .as_str()
        .filter(|s| {
            (1..=80).contains(&s.len())
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        })
        .ok_or_else(|| value_error(format!("{at}: invalid identifier or digest")))
}

pub fn review_handoff(
    work: &Path,
    assignments: &[PathBuf],
    results: &[PathBuf],
) -> Result<Value, String> {
    check_count(assignments.len(), results.len())?;
    let WorkspaceState {
        workspace:
            Workspace {
                project,
                documents: docs,
                sheets,
            },
        ..
    } = read_workspace(work)?;
    let glossary_hash = digest(&read_limited(
        &local_path(work, "glossary.json")?,
        MAX_FILE,
    )?);
    let source_hashes: HashMap<_, _> = items(&project["sources"])
        .iter()
        .map(|s| (string(&s["id"]), string(&s["sha256"])))
        .collect();
    let episodes = items(&project["episodes"]);
    let (mut assignment_map, mut covered) = (HashMap::new(), HashSet::new());
    let mut total = 0;
    for path in assignments {
        let assignment = read_envelope(path, ASSIGNMENT, &required(ASSIGNMENT, "episode"))?;
        let identifier = assignment_id(&assignment["id"], "assignment.id")?;
        if !valid_id(&assignment["source"], "s", 4) {
            return Err(value_error(
                "assignment.source: invalid identifier or digest",
            ));
        }
        let source = string(&assignment["source"]);
        if assignment_map.contains_key(identifier) || !docs.contains_key(source) {
            return Err(value_error(
                "Duplicate assignment ID or unknown assignment source",
            ));
        }
        for (field, expected) in [
            ("source_sha256", source_hashes[source]),
            ("glossary_sha256", &glossary_hash),
        ] {
            let checksum = string(&assignment[field]);
            if checksum.len() != 64
                || !checksum
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(value_error(format!(
                    "assignment.{field}: invalid identifier or digest"
                )));
            }
            if checksum != expected {
                return Err(value_error(
                    "Assignment source or glossary revision is stale",
                ));
            }
        }
        if let Some(id) = assignment.get("episode") {
            let mut mapped = episodes
                .iter()
                .filter(|episode| source_keys(episode).contains(&source));
            if mapped.next().is_none_or(|episode| &episode["id"] != id) || mapped.next().is_some() {
                return Err(value_error(
                    "Assignment episode differs from current workspace mapping",
                ));
            }
        }
        let assigned = ids(&assignment["assigned_ids"], "assigned_ids")?;
        let context = ids(&assignment["context_ids"], "context_ids")?;
        let known: HashSet<_> = docs[source]
            .cues
            .iter()
            .map(|cue| cue.id.as_str())
            .collect();
        if assigned.is_empty()
            || !assigned.is_disjoint(&context)
            || assigned.union(&context).any(|&id| !known.contains(id))
        {
            return Err(value_error("Assignment coverage/context IDs are invalid"));
        }
        for id in &assigned {
            if !covered.insert(format!("{source}:{id}")) {
                return Err(value_error("Assignments overlap"));
            }
        }
        total += assigned.len();
        if total > 250_000 {
            return Err(value_error("Handoff exceeds its total row limit"));
        }
        assignment_map.insert(identifier.to_owned(), assignment);
    }
    let mut effective: HashMap<_, _> = sheets
        .iter()
        .flat_map(|(source, rows)| {
            rows.iter()
                .map(move |row| (format!("{source}:{}", string(&row["id"])), row.clone()))
        })
        .collect();
    let (mut received, mut proposals, mut reports) = (HashSet::new(), HashMap::new(), Vec::new());
    let mut pending_total = 0;
    for path in results {
        let result = read_envelope(path, RESULT, &required(RESULT, "activity_log"))?;
        let identifier = assignment_id(&result["assignment_id"], "result.assignment_id")?;
        let assignment = assignment_map
            .get(identifier)
            .filter(|_| received.insert(identifier.to_owned()))
            .ok_or_else(|| value_error("Result assignment is unknown or duplicated"))?;
        for field in ["source_sha256", "glossary_sha256"] {
            if result[field] != assignment[field] {
                return Err(value_error("Result source or glossary revision is stale"));
            }
        }
        let state = string(&result["status"]);
        if !matches!(state, "complete" | "pending") {
            return Err(value_error("Result status must be complete or pending"));
        }
        if let Some(path) = result.get("activity_log") {
            local_path(work, string(path))?;
        }
        let rows = validation::worksheet(result["rows"].clone(), "result.rows".into())?;
        for row in &rows {
            if row
                .as_object()
                .into_iter()
                .flat_map(|map| map.keys())
                .any(|key| !ROW.contains(&key.as_str()))
            {
                return Err(value_error("Result row has an unsupported field"));
            }
        }
        let expected = ids(&assignment["assigned_ids"], "assigned_ids")?;
        if rows.len() != expected.len()
            || rows
                .iter()
                .map(|row| string(&row["id"]))
                .collect::<BTreeSet<_>>()
                != expected
        {
            return Err(value_error(
                "Result needs exactly one row per assigned cue, without context-only rows",
            ));
        }
        let source = string(&assignment["source"]);
        let doc = &docs[source];
        let originals: HashMap<_, _> = doc.cues.iter().map(|cue| (cue.id.as_str(), cue)).collect();
        let sources = episodes
            .iter()
            .map(source_keys)
            .find(|keys| keys.contains(&source))
            .unwrap_or_else(|| vec![source]);
        let known_links: HashSet<_> = sources
            .iter()
            .flat_map(|key| {
                docs[*key]
                    .cues
                    .iter()
                    .map(move |cue| format!("{key}:{}", cue.id))
            })
            .collect();
        let mut count = 0;
        for row in &rows {
            let id = string(&row["id"]);
            let key = format!("{source}:{id}");
            let cue = originals[id];
            if items(&row["links"])
                .iter()
                .any(|v| !known_links.contains(string(v)))
            {
                return Err(value_error(
                    "Result links must identify original cues in the same episode",
                ));
            }
            if row["source_text"] != cue.text {
                return Err(value_error("Result source text changed"));
            }
            if pending(row) {
                count += 1;
            } else {
                reviewed_cues(
                    &single_cue(doc, cue),
                    std::slice::from_ref(row),
                    string(&project["target_language"]),
                )?;
            }
            proposals.insert(key.clone(), sources.clone());
            effective.insert(key, row.clone());
        }
        if (state == "complete") != (count == 0) {
            return Err(value_error(
                "Result declared status differs from its pending rows",
            ));
        }
        pending_total += count;
        reports
            .push(json!({"assignment_id": identifier, "rows": rows.len(), "pending_rows": count}));
    }
    if received.len() != assignment_map.len() {
        return Err(value_error("A result is required for every assignment"));
    }
    for (key, sources) in proposals {
        let row = &effective[&key];
        if row["reviewed"] != true {
            continue;
        }
        let mut unresolved = false;
        for link in items(&row["links"]).iter().map(string) {
            let target = effective
                .get(link)
                .filter(|_| {
                    link != key
                        && link
                            .split_once(':')
                            .is_some_and(|(source, _)| sources.contains(&source))
                })
                .ok_or_else(|| value_error("Result link cannot target itself or a foreign cue"))?;
            if pending(target) {
                unresolved = true;
            } else if !matches!(string(&target["action"]), "edit" | "preserve") {
                return Err(value_error(
                    "Result links must identify effective retained cues, not excluded decisions",
                ));
            }
        }
        pending_total += usize::from(unresolved);
    }
    reports.sort_by(|a, b| string(&a["assignment_id"]).cmp(string(&b["assignment_id"])));
    bounded_report(
        json!({"version": 1, "ready": pending_total == 0, "state": if pending_total == 0 {"complete"} else {"pending"}, "assignments": reports, "pending_rows": pending_total, "coordinator_review_required": true, "linguistic_quality_certified": false}),
    )
}

mod subtitle_document {
    use crate::subtitle_formats::document::{Cue, Document};
    pub(super) fn single_cue(doc: &Document, cue: &Cue) -> Document {
        Document {
            kind: doc.kind.clone(),
            cues: vec![cue.clone()],
            ..Document::default()
        }
    }
}

fn required<'a>(fields: &[&'a str], optional: &str) -> Vec<&'a str> {
    fields
        .iter()
        .copied()
        .filter(|&key| key != optional)
        .collect()
}

fn check_count(assignments: usize, results: usize) -> Result<(), String> {
    if assignments == 0 || assignments > 1000 || results > 1000 {
        return Err(value_error(
            "Handoff needs assignments and at most 1000 files of each kind",
        ));
    }
    Ok(())
}

pub fn envelope(path: Value, fields: Value, required: Value) -> Result<Value, String> {
    let path = attr_path(&path, "absolute")?;
    let fields = iterable(&fields)?.iter().map(string).collect::<Vec<_>>();
    let required = iterable(&required)?.iter().map(string).collect::<Vec<_>>();
    read_envelope(&path, &fields, &required)
}

pub fn cue_ids(value: Value, r#where: Value) -> Result<Value, String> {
    let at = r#where.as_str().unwrap_or_else(|| python_type(&r#where));
    Ok(json!({"$set": ids(&value, at)?}))
}

pub fn handoff(work: Value, assignments: Vec<Value>, results: Vec<Value>) -> Result<Value, String> {
    check_count(assignments.len(), results.len())?;
    let work = workspace_path(&work)?;
    let paths = |values: Vec<Value>| {
        values
            .iter()
            .map(|v| attr_path(v, "absolute"))
            .collect::<Result<Vec<_>, _>>()
    };
    review_handoff(&work, &paths(assignments)?, &paths(results)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_envelope_rejects_wrong_version_and_unknown_fields() -> Result<(), String> {
        let temp = tempfile::tempdir().map_err(io_error)?;
        let path = temp.path().join("editor proposal.json");
        fs::write(&path, br#"{"version":1,"id":"review_1"}"#).map_err(io_error)?;
        assert_eq!(
            read_envelope(&path, &["version", "id"], &["version", "id"])?["id"],
            "review_1"
        );
        assert!(read_envelope(&path, &["version"], &["version"]).is_err());
        fs::write(&path, br#"{"version":true}"#).map_err(io_error)?;
        assert!(read_envelope(&path, &["version"], &["version"]).is_err());
        assert!(ids(&json!(["c000001", "c000001"]), "assigned_ids").is_err());
        Ok(())
    }
}
