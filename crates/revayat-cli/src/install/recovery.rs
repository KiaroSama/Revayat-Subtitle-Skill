use super::{files::*, journal::*, registry};
use serde_json::{Value, json};
use std::{fs, io, path::Path};

pub fn classify(record: &Value) -> io::Result<(bool, bool, bool)> {
    let (target, stage, backup, disposal) = (
        path(&record["target"])?,
        path(&record["stage"])?,
        path(&record["backup"])?,
        path(&record["disposal"])?,
    );
    let (previous, staged) = (&record["previous_id"], &record["stage_id"]);
    let old_target = matches(target, previous, &record["old"]);
    let old_backup = matches(backup, previous, &record["old"]);
    let new_target = matches(target, staged, &record["new"]);
    if existing(backup)? && !old_backup {
        return Err(invalid(
            "Recovery backup changed; preserve all transaction state",
        ));
    }
    if existing(target)? && !old_target && !new_target {
        return Err(invalid(
            "Recovery target changed; preserve all transaction state",
        ));
    }
    let stage_exists = existing(stage)?;
    if stage_exists && !matches(stage, staged, &record["new"]) {
        if json!(identity(stage)?) != *staged {
            return Err(invalid(
                "Recovery staging identity is unknown; preserve all transaction state",
            ));
        }
        tree_manifest(stage)?;
    }
    if !previous.is_null() && old_target == old_backup {
        return Err(invalid(
            "Recovery needs exactly one complete previous installation",
        ));
    }
    if existing(disposal)? {
        safe_path(disposal, true)?;
        if json!(identity(disposal)?) != *staged
            || new_target
            || !matches!(
                record["phase"].as_str(),
                Some("restore-intent" | "disposed" | "restored")
            )
        {
            return Err(invalid(
                "Recovery disposal ownership is ambiguous; preserve state",
            ));
        }
        tree_manifest(disposal)?;
    }
    if new_target && stage_exists {
        return Err(invalid(
            "Recovery has ambiguous duplicate staged/publication state",
        ));
    }
    Ok((old_target, old_backup, new_target))
}

pub fn cleanup_disposals(journal: &Path, root: &Path, home: &Path) -> io::Result<()> {
    let data = registry::load_registered(root, journal, home)?;
    if data["state"] != "restored" {
        return Err(invalid("Disposal cleanup requires a restored transaction"));
    }
    cleanup_owned_disposals(&data)?;
    registry::unregister(root, journal, home)
}

fn cleanup_owned_disposals(data: &Value) -> io::Result<()> {
    let records = data["records"]
        .as_array()
        .ok_or_else(|| invalid("Invalid installation records"))?;
    for record in records {
        for field in ["disposal", "stage"] {
            let disposal = path(&record[field])?;
            let attempt = || -> io::Result<()> {
                if !existing(disposal)? {
                    return Ok(());
                }
                safe_path(disposal, true)?;
                if record["phase"] != "restored" || json!(identity(disposal)?) != record["stage_id"]
                {
                    return Err(invalid(
                        "Recovery disposal identity changed; preserve state",
                    ));
                }
                tree_manifest(disposal)?;
                if json!(identity(disposal)?) != record["stage_id"] {
                    return Err(invalid(
                        "Recovery disposal identity changed; preserve state",
                    ));
                }
                fs::remove_dir_all(disposal)?;
                if let Some(parent) = disposal.parent() {
                    sync_directory(parent)?;
                }
                Ok(())
            };
            // Failed cleanup stays registered without touching live targets.
            let _ = attempt();
        }
    }
    Ok(())
}

pub fn restore(journal: &Path, root: &Path, home: &Path, explicit: bool) -> io::Result<bool> {
    restore_with(journal, root, home, explicit, move_owned)
}

pub fn restore_with(
    journal: &Path,
    root: &Path,
    home: &Path,
    explicit: bool,
    rename: impl Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<bool> {
    let mut data = registry::load_registered(root, journal, home)?;
    match data["state"].as_str() {
        Some("committed") if explicit => {
            return Err(invalid(
                "Committed installation is not interrupted; --recover does not authorize uninstall",
            ));
        }
        Some("committed") => return Ok(false),
        Some("restored") => (),
        _ => {
            let positions = records(&data)?
                .iter()
                .map(classify)
                .collect::<io::Result<Vec<_>>>()?;
            data["state"] = json!("restoring");
            registry::save_journal(root, journal, &data)?;
            for (i, (_, old_backup, new_target)) in positions.into_iter().enumerate().rev() {
                data["records"][i]["phase"] = json!("restore-intent");
                registry::save_journal(root, journal, &data)?;
                let record = data["records"][i].clone();
                classify(&record)?;
                let (target, backup, disposal) = (
                    path(&record["target"])?,
                    path(&record["backup"])?,
                    path(&record["disposal"])?,
                );
                if new_target {
                    if !matches(target, &record["stage_id"], &record["new"]) || existing(disposal)?
                    {
                        return Err(invalid(
                            "Recovery publication or disposal changed before removal",
                        ));
                    }
                    rename(target, disposal)?;
                    data["records"][i]["phase"] = json!("disposed");
                    registry::save_journal(root, journal, &data)?;
                }
                if old_backup {
                    if existing(target)? || !matches(backup, &record["previous_id"], &record["old"])
                    {
                        return Err(invalid("Recovery old-tree destination changed"));
                    }
                    rename(backup, target)?;
                }
                if record["previous_id"].is_null() {
                    if existing(target)? {
                        return Err(invalid("Recovery did not restore original absence"));
                    }
                } else if !matches(target, &record["previous_id"], &record["old"]) {
                    return Err(invalid("Recovery did not restore complete previous bytes"));
                }
                data["records"][i]["phase"] = json!("restored");
                registry::save_journal(root, journal, &data)?;
            }
            data["state"] = json!("restored");
            registry::save_journal(root, journal, &data)?;
        }
    }
    cleanup_disposals(journal, root, home)?;
    Ok(true)
}

pub fn recover(journal: &Path, recovery: &Path, home: &Path) -> io::Result<bool> {
    let (root, _lock) = lock_owner(home)?;
    recover_owned(journal, recovery, &root, home)
}

pub fn recover_owned(
    journal: &Path,
    recovery: &Path,
    root: &Path,
    home: &Path,
) -> io::Result<bool> {
    let journal = safe_path(journal, false)?;
    let data = registry::load_registered(root, &journal, home)?;
    if safe_path(recovery, true)? != path(&data["recovery"])? {
        return Err(invalid("--recovery-dir differs from the journal root"));
    }
    restore(&journal, root, home, true)
}
