use super::files::*;
use serde_json::{Value, json};
use std::{
    io,
    path::{Component, Path, PathBuf},
};

pub fn object_fields(v: &Value, keys: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|m| m.len() == keys.len() && keys.iter().all(|key| m.contains_key(*key)))
}

pub fn text(v: &Value) -> io::Result<&str> {
    v.as_str()
        .filter(|p| !p.is_empty() && !p.contains('\0'))
        .ok_or_else(|| invalid("Invalid installation journal path field"))
}

pub fn path(v: &Value) -> io::Result<&Path> {
    Ok(Path::new(text(v)?))
}

pub fn records(v: &Value) -> io::Result<&[Value]> {
    v["records"]
        .as_array()
        .filter(|r| (1..=1000).contains(&r.len()))
        .map(Vec::as_slice)
        .ok_or_else(|| invalid("Installation journal target limit is invalid"))
}

pub fn existing(path: &Path) -> io::Result<bool> {
    Ok(metadata(path)?.is_some())
}

pub(super) fn path_key(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

pub fn overlaps(a: &Path, b: &Path) -> bool {
    let (a, b) = (path_key(a), path_key(b));
    a.starts_with(&b) || b.starts_with(&a)
}

pub(super) fn canonical(path: &Path) -> bool {
    path.is_absolute()
        && !path.components().any(|c| c == Component::ParentDir)
        && path.components().collect::<PathBuf>().as_os_str() == path.as_os_str()
}

pub fn disposals_pending(data: &Value) -> io::Result<bool> {
    let records = data["records"]
        .as_array()
        .ok_or_else(|| invalid("Invalid installation records"))?;
    for record in records {
        for field in ["disposal", "stage"] {
            if !matches!(metadata(path(&record[field])?), Ok(None)) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub fn load_journal(journal: &Path, home: &Path) -> io::Result<Value> {
    let journal = safe_path(journal, false)?;
    validate_journal(&journal, home, read_json(&journal)?)
}

pub(super) fn validate_journal(journal: &Path, home: &Path, data: Value) -> io::Result<Value> {
    let id = data["id"].as_str().filter(|s| is_hex(s, 32));
    let state = data["state"].as_str();
    if !object_fields(
        &data,
        &[
            "version",
            "id",
            "owner",
            "recovery",
            "state",
            "records",
            "ancestors",
        ],
    ) || data["version"].as_u64() != Some(1)
        || !matches!(
            state,
            Some("staging" | "staged" | "restoring" | "restored" | "committed")
        )
        || data["owner"].as_str() != safe_path(home, true)?.to_str()
    {
        return Err(invalid(
            "Unknown or foreign installation journal; preserve state",
        ));
    }
    let id =
        id.ok_or_else(|| invalid("Unknown or foreign installation journal; preserve state"))?;
    let recovery = safe_path(path(&data["recovery"])?, true)?;
    let transaction = recovery.join(format!("transaction-{id}"));
    if journal != transaction.join("journal.json") {
        return Err(invalid(
            "Installation journal is outside its declared recovery root",
        ));
    }
    let terminal = matches!(state, Some("committed" | "restored"));
    let mut targets = Vec::new();
    for (index, record) in records(&data)?.iter().enumerate() {
        if !object_fields(
            record,
            &[
                "target",
                "stage",
                "backup",
                "disposal",
                "previous_id",
                "stage_id",
                "old",
                "new",
                "phase",
            ],
        ) {
            return Err(invalid("Installation journal record fields are invalid"));
        }
        let target = path(&record["target"])?;
        if !terminal {
            safe_path(target, true)?;
        }
        if !canonical(target) || targets.iter().any(|old: &&Path| overlaps(old, target)) {
            return Err(invalid(
                "Installation journal targets are noncanonical or overlap",
            ));
        }
        targets.push(target);
        for (field, prefix) in [("stage", "new"), ("backup", "old"), ("disposal", "dispose")] {
            if path(&record[field])?.as_os_str()
                != transaction.join(format!("{prefix}-{index:04}")).as_os_str()
            {
                return Err(invalid(
                    "Installation journal stage/backup paths are invalid",
                ));
            }
        }
        for (field, message) in [
            ("previous_id", "Invalid previous installation identity"),
            ("stage_id", "Invalid staged installation identity"),
        ] {
            if !record[field].is_null() && !valid_identity(&record[field]) {
                return Err(invalid(message));
            }
        }
        if !matches!(
            record["phase"].as_str(),
            Some(
                "staging"
                    | "staged"
                    | "old-intent"
                    | "old-moved"
                    | "new-intent"
                    | "published"
                    | "restore-intent"
                    | "disposed"
                    | "restored"
            )
        ) {
            return Err(invalid("Invalid installation journal phase"));
        }
        if !record["old"].is_null() {
            validate_manifest(&record["old"])?;
        }
        if record["new"].is_null() {
            if record["old"].is_null() || !record["stage_id"].is_null() {
                return Err(invalid("Invalid uninstall journal record"));
            }
        } else {
            validate_manifest(&record["new"])?;
        }
        if record["old"].is_null() != record["previous_id"].is_null() {
            return Err(invalid("Previous installation identity/manifest mismatch"));
        }
    }
    let ancestors = data["ancestors"]
        .as_object()
        .filter(|a| a.len() <= 10_000)
        .ok_or_else(|| invalid("Invalid installation ancestor inventory"))?;
    if terminal
        && transaction
            .ancestors()
            .any(|p| p.to_str().is_none_or(|s| !ancestors.contains_key(s)))
    {
        return Err(invalid(
            "Installation recovery ancestor inventory is incomplete; preserve state",
        ));
    }
    for (name, expected) in ancestors {
        let ancestor = Path::new(name);
        let error = || invalid("Installation recovery ancestor changed; preserve state");
        if !canonical(ancestor) || !valid_identity(expected) {
            return Err(error());
        }
        if terminal && !transaction.ancestors().any(|p| p == ancestor) {
            continue;
        }
        if safe_path(ancestor, true)?.as_os_str() != ancestor.as_os_str()
            || json!(identity(ancestor)?) != *expected
        {
            return Err(error());
        }
    }
    Ok(data)
}
