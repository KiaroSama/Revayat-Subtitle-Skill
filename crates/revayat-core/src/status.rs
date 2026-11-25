//! Read-only workspace and visual review inventory.

#![forbid(unsafe_code)]

use crate::render::delivery;
use crate::runtime::files::local_path;
use crate::validation;
use crate::workflow::contract::{io_error, items, linked, string, value_error, workspace_path};
use crate::workflow::workspace::{
    self, Workspace, episode_identity, read_record, source_keys, utf8_path,
};
use crate::workflow::{docs_value, review};
use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashMap, HashSet, hash_map::Entry};
use std::fs;
use std::path::Path;

const MAX_REPORT_BYTES: usize = 1024 * 1024;

pub struct WorkspaceState {
    pub workspace: Workspace,
    pub glossary: Value,
    pub unassigned: Vec<String>,
}

pub fn read_workspace(work: &Path) -> Result<WorkspaceState, String> {
    let workspace = workspace::load(work)?;
    let Workspace {
        project,
        documents: docs,
        sheets,
    } = &workspace;
    let glossary =
        validation::glossary(read_record(work, "glossary.json")?, "glossary.json".into())?;
    if glossary["series"] != project["series"] {
        return Err(value_error(
            "Glossary belongs to a different series identity",
        ));
    }
    for (key, doc) in docs {
        let rows = &sheets[key];
        if !rows
            .iter()
            .map(|row| string(&row["id"]))
            .eq(doc.cues.iter().map(|cue| cue.id.as_str()))
        {
            return Err(value_error(format!(
                "Worksheet {key}: original cue coverage/order differs"
            )));
        }
        if rows
            .iter()
            .zip(&doc.cues)
            .any(|(row, cue)| row["source_text"] != cue.text)
        {
            return Err(value_error(format!("Worksheet {key}: source text differs")));
        }
    }
    let mut names = HashSet::new();
    let mut assigned = BTreeSet::new();
    let mut duplicate = false;
    for episode in items(&project["episodes"]) {
        let identity = episode_identity(string(&episode["id"]))
            .filter(|(season, _, index)| project["season"] == *season && *index >= 1);
        if identity.is_none_or(|value| !names.insert(value))
            || string(&episode["comparison"]).trim().is_empty()
        {
            return Err(value_error(
                "Episode assignment identity or comparison is invalid",
            ));
        }
        let sources = source_keys(episode);
        if sources.iter().any(|key| !docs.contains_key(*key)) {
            return Err(value_error(
                "Episode assignment references an unknown source",
            ));
        }
        for key in sources {
            duplicate |= !assigned.insert(key);
        }
    }
    if duplicate {
        return Err(value_error(
            "A source belongs to more than one episode assignment",
        ));
    }
    let unassigned = docs
        .keys()
        .filter(|key| !assigned.contains(key.as_str()))
        .cloned()
        .collect();
    Ok(WorkspaceState {
        workspace,
        glossary,
        unassigned,
    })
}

pub fn workspace_state(work: Value) -> Result<Value, String> {
    let WorkspaceState {
        workspace:
            Workspace {
                project,
                documents,
                sheets,
            },
        glossary,
        unassigned,
    } = read_workspace(&workspace_path(&work)?)?;
    let docs = docs_value(documents, Some(&project))?;
    let ordered: Map<_, _> = docs
        .as_object()
        .into_iter()
        .flat_map(Map::keys)
        .map(|key| (key.clone(), json!(sheets[key])))
        .collect();
    Ok(json!({"$tuple": [project, docs, ordered, glossary, unassigned]}))
}

pub fn bounded_report(report: Value) -> Result<Value, String> {
    if crate::workflow::contract_json::dump(&report, false, false)?.len() > MAX_REPORT_BYTES {
        return Err(value_error("Read-only report exceeds its 1 MiB byte limit"));
    }
    Ok(report)
}

pub fn report(work: &Path) -> Result<Value, String> {
    let WorkspaceState {
        workspace:
            Workspace {
                project,
                documents: docs,
                sheets,
            },
        glossary,
        unassigned,
    } = read_workspace(work)?;
    let mut pending = 0;
    let mut first_pending = None;
    for source in items(&project["sources"]) {
        let key = string(&source["id"]);
        for row in &sheets[key] {
            if workspace::pending(row) {
                pending += 1;
                first_pending.get_or_insert_with(|| json!({"source": key, "cue": row["id"]}));
            }
        }
    }
    let glossary_ready = glossary["terms_reviewed"] == true;
    if glossary_ready {
        review::verify_glossary(glossary, string(&project["series"]))?;
    }
    let complete = pending == 0
        && glossary_ready
        && unassigned.is_empty()
        && !items(&project["episodes"]).is_empty();
    let mut expected_by_format = HashMap::new();
    let mut editions = Vec::new();
    let root = local_path(work, "builds")?;
    let mut ready = false;
    if root.exists() {
        if !root.is_dir() {
            return Err(value_error("Edition inventory must be a directory"));
        }
        for entry in fs::read_dir(&root).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
            if editions.len() >= 1000 || !metadata.is_dir() || linked(&metadata) {
                return Err(value_error(
                    "Edition inventory exceeds its limit or contains a non-directory",
                ));
            }
            let name = entry.file_name();
            let name = utf8_path(Path::new(&name))?;
            let path = local_path(work, &format!("builds/{name}"))?;
            if !hash(&json!(name)) {
                return Err(value_error(
                    "Edition inventory contains an invalid identity",
                ));
            }
            let manifest = validation::obj(
                read_record(&path, "manifest.json")?,
                "edition manifest".into(),
            )?;
            if manifest["identity"] != name {
                return Err(value_error(
                    "Edition manifest identity differs from its directory",
                ));
            }
            if !matches!(manifest["version"].as_i64(), Some(1 | 2)) {
                return Err(value_error("Edition manifest schema is invalid"));
            }
            validation::string(manifest["series"].clone(), "edition.series".into(), false)?;
            for episode in validation::array(
                manifest["episodes"].clone(),
                "edition.episodes".into(),
                1000,
            )? {
                let episode = validation::obj(episode, "edition.episode".into())?;
                validation::string(episode["id"].clone(), "edition.episode.id".into(), false)?;
                validation::string(
                    episode["file"].clone(),
                    "edition.episode.file".into(),
                    false,
                )?;
                if !hash(&episode["sha256"]) {
                    return Err(value_error(
                        "edition.episode.sha256: invalid identifier or digest",
                    ));
                }
            }
            let mut item =
                json!({"identity": name, "state": "stale", "episodes": 0, "visual_pending": 0});
            let format = string(&manifest["output_format"]);
            workspace::format(format)?;
            let mut matches = false;
            if complete {
                let expected = match expected_by_format.entry(format.to_owned()) {
                    Entry::Occupied(entry) => entry.into_mut(),
                    Entry::Vacant(entry) => entry.insert(
                        workspace::assemble(work, Some(format))?.manifest["identity"].clone(),
                    ),
                };
                matches = *expected == name;
            }
            if matches {
                let (verified, _) = delivery::load_build(&path)?;
                let episodes = items(&verified["episodes"]);
                let mut existing = HashSet::new();
                let mut missing = 0;
                let mut visual_pending = 0;
                for episode in episodes {
                    let id = string(&episode["id"]);
                    let filename = format!("renders/{id}.json");
                    if !local_path(&path, &filename)?.exists() {
                        missing += 1;
                        continue;
                    }
                    let evidence = validation::render_record(
                        read_record(&path, &filename)?,
                        "render review".into(),
                    )?;
                    existing.insert(id);
                    visual_pending += items(&evidence["frames"])
                        .iter()
                        .filter(|frame| {
                            frame["reviewed"] != true || string(&frame["note"]).trim().is_empty()
                        })
                        .count();
                }
                if !existing.is_empty() && (visual_pending != 0 || missing != 0) {
                    delivery::check_delivery(&path, false, Some(&existing))?;
                }
                let edition_ready = visual_pending == 0 && missing == 0;
                if edition_ready {
                    delivery::check_delivery(&path, true, None)?;
                    ready = true;
                }
                item["episodes"] = json!(episodes.len());
                item["visual_pending"] = json!(visual_pending + missing);
                item["state"] = json!(if edition_ready { "complete" } else { "pending" });
            }
            editions.push(item);
        }
    }
    editions.sort_by(|a, b| string(&a["identity"]).cmp(string(&b["identity"])));
    bounded_report(
        json!({"version": 1, "ready": ready, "state": if ready {"complete"} else {"pending"}, "sources": docs.len(), "cues": docs.values().map(|doc| doc.cues.len()).sum::<usize>(), "pending_cues": pending, "first_pending": first_pending, "glossary": if glossary_ready {"complete"} else {"pending"}, "unassigned_sources": unassigned, "editions": editions, "linguistic_quality_certified": false}),
    )
}

fn hash(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

pub fn status(work: Value) -> Result<Value, String> {
    report(&workspace_path(&work)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_limit_counts_utf8_bytes_and_python_separators() -> Result<(), String> {
        let text = "گ".repeat((MAX_REPORT_BYTES - 2) / 2);
        assert_eq!(bounded_report(json!(text))?, text);
        assert_eq!(
            bounded_report(json!([text])).err().as_deref(),
            Some("ValueError: Read-only report exceeds its 1 MiB byte limit")
        );
        Ok(())
    }
}
