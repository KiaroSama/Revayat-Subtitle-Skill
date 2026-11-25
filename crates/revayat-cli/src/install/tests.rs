use super::{distribution, files, journal, recovery, registry, transaction};
use serde_json::{Value, json};
use std::{fs, io, path::PathBuf};

#[test]
fn install_then_uninstall_retains_complete_previous_tree() -> io::Result<()> {
    let temp = tempfile::tempdir()?;
    let checkout = temp.path().join("source");
    let skill = checkout.join("skills/revayat-subtitle");
    fs::create_dir_all(skill.join("voice/data"))?;
    fs::write(skill.join("SKILL.md"), "سلام")?;
    fs::write(skill.join("voice/data/glossary.tsv"), b"a\tb\n")?;
    let target = temp.path().join("project/skills/revayat-subtitle");
    let holding = temp.path().join("recovery");
    let targets = [target.clone()];
    transaction::execute_plan(
        transaction::plan_install(&checkout, &targets, false, false, Some(&holding), false)?,
        temp.path(),
    )?;
    let backup = transaction::execute_plan(
        transaction::plan_uninstall(&checkout, &targets, false, Some(&holding), false)?,
        temp.path(),
    )?;
    assert!(!target.exists());
    let saved = backup[0]
        .as_ref()
        .ok_or_else(|| io::Error::other("missing backup"))?;
    assert_eq!(fs::read_to_string(saved.join("SKILL.md"))?, "سلام");
    assert_eq!(fs::read(saved.join("voice/data/glossary.tsv"))?, b"a\tb\n");
    Ok(())
}

#[test]
fn rollback_reclaims_owned_disposal_under_legacy_capacity_pressure() -> io::Result<()> {
    let temp = tempfile::tempdir()?;
    let home = temp.path();
    let (owner, _lock) = files::lock_owner(home)?;
    let id = "0123456789abcdef0123456789abcdef";
    let holding = home.join("recovery");
    let transaction = holding.join(format!("transaction-{id}"));
    let journal_path = transaction.join("journal.json");
    let live_parent = home.join("project");
    let disposal = transaction.join("dispose-0000");
    fs::create_dir_all(&disposal)?;
    fs::create_dir(&live_parent)?;
    fs::write(disposal.join("SKILL.md"), b"owned")?;
    let ancestors: serde_json::Map<String, Value> = transaction
        .ancestors()
        .chain(std::iter::once(live_parent.as_path()))
        .map(|path| {
            Ok((
                path.to_string_lossy().into_owned(),
                json!(files::identity(path)?),
            ))
        })
        .collect::<io::Result<_>>()?;
    files::write_json(
        &journal_path,
        &json!({
            "version": 1, "id": id, "owner": home, "recovery": holding, "state": "restored",
            "records": [{"target": live_parent.join("skill"), "stage": transaction.join("new-0000"),
                "backup": transaction.join("old-0000"), "disposal": disposal,
                "previous_id": null, "stage_id": files::identity(&disposal)?,
                "old": null, "new": files::tree_manifest(&disposal)?, "phase": "restored"}],
            "ancestors": ancestors
        }),
    )?;
    let mut records = vec![
        json!({"path":journal_path, "transaction_id": files::identity(&transaction)?,
        "digests": [files::digest(&fs::read(&journal_path)?)], "active":false}),
    ];
    records.extend((0..1000).map(|i| {
        json!({"path":home.join(format!("unused-{i}")).join("journal.json"),
        "transaction_id":["1","2"],"digests":["0".repeat(64)],"active":false})
    }));
    files::write_json(&owner.join("pending.json"), &json!(records))?;
    assert_eq!(registry::registry(&owner)?.len(), 1001);
    fs::remove_dir(&live_parent)?;
    let held = transaction.join("held-disposal");
    fs::rename(&disposal, &held)?;
    fs::create_dir(&disposal)?;
    fs::write(disposal.join("foreign.txt"), b"preserve me")?;
    assert!(recovery::recover_owned(
        &journal_path,
        &holding,
        &owner,
        home
    )?);
    assert_eq!(fs::read(disposal.join("foreign.txt"))?, b"preserve me");
    assert!(registry::registered(&owner, &journal_path)?.active);
    fs::remove_file(disposal.join("foreign.txt"))?;
    fs::remove_dir(&disposal)?;
    fs::rename(&held, &disposal)?;
    assert!(recovery::recover_owned(
        &journal_path,
        &holding,
        &owner,
        home
    )?);
    assert!(!disposal.exists());
    assert!(!live_parent.exists());
    assert_eq!(registry::registry(&owner)?.len(), 1000);
    assert_eq!(
        journal::load_journal(&journal_path, home)?["state"],
        "restored"
    );
    let pending = owner.join("pending.json");
    let mut signed = files::read_json(&pending)?;
    signed["records"][0]["active"] = json!(true);
    files::write_json(&pending, &signed)?;
    assert!(registry::registry(&owner).is_err());
    files::write_json(&pending, &json!(records))?;
    assert!(registry::registry(&owner).is_err());
    Ok(())
}

#[test]
fn owner_lock_and_exclusive_move_do_not_replace_live_paths() -> io::Result<()> {
    let temp = tempfile::tempdir()?;
    let (root, lock) = files::lock_owner(temp.path())?;
    let lock_path = root.join("owner.lock");
    let before = files::identity(&lock_path)?;
    assert!(files::lock_owner(temp.path()).is_err());
    drop(lock);
    let (_, _lock) = files::lock_owner(temp.path())?;
    assert_eq!(files::identity(&lock_path)?, before);
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    fs::create_dir(&source)?;
    fs::create_dir(&destination)?;
    assert_eq!(
        files::move_owned(&source, &destination).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert!(source.is_dir() && destination.is_dir());
    Ok(())
}

#[cfg(unix)]
#[test]
fn dangling_links_are_not_install_paths() -> io::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("link");
    std::os::unix::fs::symlink(temp.path().join("absent"), &path)?;
    assert!(files::safe_path(&path, false).is_err());
    assert!(files::safe_path(&path.join("child"), true).is_err());
    assert!(files::tree_manifest(temp.path()).is_err());
    Ok(())
}

#[test]
fn agent_targets_include_nine_hosts_and_all() {
    let base = PathBuf::from("root");
    assert_eq!(distribution::AGENTS.len() + 1, 10);
    for agent in distribution::AGENTS {
        for scope in ["user", "project"] {
            assert!(
                distribution::destination(agent, scope, &base)
                    .unwrap()
                    .ends_with("skills/revayat-subtitle")
            );
        }
    }
}
