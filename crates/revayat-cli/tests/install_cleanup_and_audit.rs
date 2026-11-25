use anyhow::Result;
use hmac::{Hmac, KeyInit, Mac};
use revayat_cli::install::{files, recovery, registry, transaction};
use serde_json::{Value, json};
use sha2::Sha256;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

struct Account {
    _lock: fs::File,
    _scratch: tempfile::TempDir,
    home: PathBuf,
    state: PathBuf,
    source: PathBuf,
    holding: PathBuf,
}

impl Account {
    fn new() -> Result<Self> {
        let scratch = tempfile::tempdir()?;
        let root = files::safe_path(scratch.path(), true)?;
        let home = root.join("home");
        let source = root.join("checkout");
        let holding = root.join("retained history");
        fs::create_dir(&home)?;
        let (state, lock) = files::lock_owner(&home)?;
        let skill = source.join("skills/revayat-subtitle");
        fs::create_dir_all(&skill)?;
        fs::write(skill.join("SKILL.md"), b"bundled skill")?;
        Ok(Self {
            _scratch: scratch,
            home,
            state,
            _lock: lock,
            source,
            holding,
        })
    }

    fn pending(&self) -> PathBuf {
        self.state.join("pending.json")
    }

    fn install(&self, target: &Path, force: bool) -> Result<Option<PathBuf>> {
        let plan = transaction::plan_install(
            &self.source,
            &[target.into()],
            false,
            force,
            Some(&self.holding),
            false,
        )?;
        Ok(transaction::execute_owned(plan, &self.state, &self.home)?.remove(0))
    }

    fn uninstall(&self, target: &Path) -> Result<Option<PathBuf>> {
        let plan = transaction::plan_uninstall(
            &self.source,
            &[target.into()],
            false,
            Some(&self.holding),
            false,
        )?;
        Ok(transaction::execute_owned(plan, &self.state, &self.home)?.remove(0))
    }

    fn restored(&self, disposal: bool) -> Result<(PathBuf, PathBuf, PathBuf)> {
        let id = "0123456789abcdef0123456789abcdef";
        let tx = self.holding.join(format!("transaction-{id}"));
        let target = self.home.join("live").join("skill");
        let parent = target.parent().expect("live parent");
        let discard = tx.join("dispose-0000");
        fs::create_dir_all(parent)?;
        fs::create_dir_all(&discard)?;
        fs::write(discard.join("SKILL.md"), b"owned disposal")?;
        let ancestors: serde_json::Map<String, Value> = tx
            .ancestors()
            .chain(parent.ancestors())
            .map(|path| {
                Ok((
                    path.to_string_lossy().into_owned(),
                    json!(files::identity(path)?),
                ))
            })
            .collect::<io::Result<_>>()?;
        let journal = tx.join("journal.json");
        files::write_json(
            &journal,
            &json!({"version":1,"id":id,"owner":self.home,"recovery":self.holding,
            "state":"restored","records":[{"target":target,"stage":tx.join("new-0000"),
            "backup":tx.join("old-0000"),"disposal":discard,"previous_id":null,
            "stage_id":files::identity(&discard)?,"old":null,"new":files::tree_manifest(&discard)?,
            "phase":"restored"}], "ancestors":ancestors}),
        )?;
        if !disposal {
            fs::remove_dir_all(&discard)?;
        }
        Ok((journal, discard, target))
    }

    fn legacy(&self, journal: Option<&Path>, filler: usize, active: bool) -> Result<()> {
        let mut rows = Vec::new();
        if let Some(journal) = journal {
            rows.push(json!({"path":journal,"transaction_id":files::identity(journal.parent().expect("transaction"))?,
                "digests":[files::digest(&fs::read(journal)?)], "active":false}));
        }
        rows.extend((0..filler).map(|i| {
            json!({"path":self.home.join(format!("historical-{i}")).join("journal.json"),
            "transaction_id":["1","2"],"digests":["0".repeat(64)], "active":active})
        }));
        files::write_json(&self.pending(), &json!(rows))?;
        Ok(())
    }

    fn recover(&self, journal: &Path) -> io::Result<bool> {
        recovery::recover_owned(journal, &self.holding, &self.state, &self.home)
    }
}

#[test]
fn installation_authenticates_the_exact_registry_rows() -> Result<()> {
    let a = Account::new()?;
    a.install(&a.home.join("new skill"), false)?;
    let registry = files::read_json(&a.pending())?;
    let key = fs::read(a.state.join("registry.key"))?;
    assert_eq!(key.len(), 32);
    let mut mac = Hmac::<Sha256>::new_from_slice(&key)?;
    mac.update(b"revayat-install-registry-v1\0");
    mac.update(&serde_json::to_vec(&registry["records"])?);
    mac.verify_slice(&hex::decode(
        registry["hmac"].as_str().expect("registry tag"),
    )?)?;
    assert_eq!(registry["records"][0]["active"], false);
    Ok(())
}

fn tamper(mutate: impl FnOnce(&mut Value)) -> Result<()> {
    let a = Account::new()?;
    let installed = a.home.join("installed");
    a.install(&installed, false)?;
    let mut signed = files::read_json(&a.pending())?;
    mutate(&mut signed);
    files::write_json(&a.pending(), &signed)?;
    let before = fs::read(a.pending())?;
    let refused = a.home.join("refused");
    let error = a
        .install(&refused, false)
        .expect_err("authentication must fail closed");
    assert!(
        error.to_string().contains("authentication failed"),
        "{error}"
    );
    assert_eq!(fs::read(a.pending())?, before);
    assert_eq!(fs::read(installed.join("SKILL.md"))?, b"bundled skill");
    assert!(!refused.exists());
    Ok(())
}

#[test]
fn active_bit_is_authenticated() -> Result<()> {
    tamper(|v| v["records"][0]["active"] = json!(true))
}
#[test]
fn pathname_is_authenticated() -> Result<()> {
    tamper(|v| v["records"][0]["path"] = json!("elsewhere"))
}
#[test]
fn journal_digest_is_authenticated() -> Result<()> {
    tamper(|v| v["records"][0]["digests"][0] = json!("0".repeat(64)))
}
#[test]
fn incorrect_tag_is_rejected() -> Result<()> {
    tamper(|v| v["hmac"] = json!("f".repeat(64)))
}
#[test]
fn registry_downgrade_is_rejected_once_a_key_exists() -> Result<()> {
    tamper(|v| *v = v["records"].take())
}
#[test]
fn unknown_signed_envelope_field_is_rejected() -> Result<()> {
    tamper(|v| v["unknown"] = json!(true))
}

#[test]
fn missing_signed_registry_is_not_treated_as_empty_history() -> Result<()> {
    let a = Account::new()?;
    a.install(&a.home.join("first"), false)?;
    fs::remove_file(a.pending())?;
    assert!(a.install(&a.home.join("second"), false).is_err());
    assert!(!a.home.join("second").exists());
    Ok(())
}

#[test]
fn damaged_authentication_key_does_not_reinitialize_history() -> Result<()> {
    let a = Account::new()?;
    a.install(&a.home.join("first"), false)?;
    let key = a.state.join("registry.key");
    fs::write(&key, b"not a key")?;
    assert!(registry::registry(&a.state).is_err());
    assert_eq!(fs::read(key)?, b"not a key");
    Ok(())
}

#[test]
fn capacity_retires_inactive_history_and_accepts_a_new_install() -> Result<()> {
    let a = Account::new()?;
    let (journal, _, _) = a.restored(false)?;
    a.legacy(Some(&journal), 999, false)?;
    assert_eq!(registry::registry(&a.state)?.len(), 1000);
    let target = a.home.join("new");
    a.install(&target, false)?;
    let rows = registry::registry(&a.state)?;
    assert_eq!(rows.len(), 1000);
    assert!(rows.iter().all(|row| row.path != journal));
    assert!(journal.is_file());
    assert_eq!(fs::read(target.join("SKILL.md"))?, b"bundled skill");
    Ok(())
}

#[test]
fn all_active_capacity_refuses_admission_without_mutation() -> Result<()> {
    let a = Account::new()?;
    let (journal, _, _) = a.restored(false)?;
    a.legacy(None, 1000, true)?;
    let before = fs::read(a.pending())?;
    let error = registry::register(&a.state, &journal).expect_err("all entries are active");
    assert!(error.to_string().contains("registry is full"));
    assert_eq!(fs::read(a.pending())?, before);
    assert!(!a.state.join("registry.key").exists());
    Ok(())
}

#[test]
fn legacy_overflow_is_readable_for_recovery() -> Result<()> {
    let a = Account::new()?;
    let (journal, discard, _) = a.restored(true)?;
    a.legacy(Some(&journal), 1000, false)?;
    assert_eq!(registry::registry(&a.state)?.len(), 1001);
    assert!(a.recover(&journal)?);
    assert!(!discard.exists());
    assert_eq!(registry::registry(&a.state)?.len(), 1000);
    assert!(!registry::registered(&a.state, &journal)?.active);
    assert!(a.state.join("registry.key").is_file());
    Ok(())
}

#[test]
fn more_than_one_legacy_overflow_row_is_refused() -> Result<()> {
    let a = Account::new()?;
    a.legacy(None, 1002, false)?;
    let before = fs::read(a.pending())?;
    assert!(registry::registry(&a.state).is_err());
    assert_eq!(fs::read(a.pending())?, before);
    Ok(())
}

#[test]
fn failed_disposal_is_preserved_and_retryable_under_capacity_pressure() -> Result<()> {
    let a = Account::new()?;
    let (journal, discard, target) = a.restored(true)?;
    a.legacy(Some(&journal), 1000, false)?;
    let held = discard.with_extension("owned");
    fs::rename(&discard, &held)?;
    fs::create_dir(&discard)?;
    fs::write(discard.join("unrelated"), b"foreign data")?;
    fs::create_dir(&target)?;
    fs::write(target.join("mine"), b"live data")?;
    assert!(a.recover(&journal)?);
    assert_eq!(fs::read(discard.join("unrelated"))?, b"foreign data");
    assert!(registry::registered(&a.state, &journal)?.active);
    assert_eq!(registry::registry(&a.state)?.len(), 1000);
    a.install(&a.home.join("another skill"), false)?;
    assert!(registry::registered(&a.state, &journal)?.active);
    fs::remove_dir_all(&discard)?;
    fs::rename(&held, &discard)?;
    assert!(a.recover(&journal)?);
    assert!(!discard.exists());
    assert!(!registry::registered(&a.state, &journal)?.active);
    assert_eq!(fs::read(target.join("mine"))?, b"live data");
    Ok(())
}

#[test]
fn terminal_cleanup_survives_relocated_live_parent() -> Result<()> {
    let a = Account::new()?;
    let (journal, discard, target) = a.restored(true)?;
    registry::register(&a.state, &journal)?;
    let parent = target.parent().expect("live parent");
    let moved = parent.with_extension("relocated");
    fs::write(parent.join("local data"), b"preserve")?;
    fs::rename(parent, &moved)?;
    assert!(a.recover(&journal)?);
    assert!(!discard.exists());
    assert!(!parent.exists());
    assert_eq!(fs::read(moved.join("local data"))?, b"preserve");
    Ok(())
}

#[test]
fn terminal_recovery_root_replacement_refuses_cleanup() -> Result<()> {
    let a = Account::new()?;
    let (journal, discard, _) = a.restored(true)?;
    registry::register(&a.state, &journal)?;
    let tx = journal.parent().expect("transaction");
    let moved = tx.with_extension("relocated");
    fs::rename(tx, &moved)?;
    fs::create_dir(tx)?;
    fs::copy(moved.join("journal.json"), &journal)?;
    assert!(a.recover(&journal).is_err());
    assert_eq!(
        fs::read(
            moved
                .join(discard.file_name().expect("disposal"))
                .join("SKILL.md")
        )?,
        b"owned disposal"
    );
    Ok(())
}

#[test]
fn dangling_disposal_link_preserves_legacy_cleanup_authority() -> Result<()> {
    let a = Account::new()?;
    let (journal, discard, _) = a.restored(false)?;
    let missing = a.home.join("missing");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&missing, &discard)?;
    #[cfg(windows)]
    {
        fs::create_dir(&missing)?;
        let created = std::process::Command::new(
            std::env::var_os("COMSPEC").unwrap_or_else(|| "cmd.exe".into()),
        )
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&discard)
        .arg(&missing)
        .stdin(std::process::Stdio::null())
        .output()?;
        assert!(
            created.status.success(),
            "{}",
            String::from_utf8_lossy(&created.stderr)
        );
        fs::remove_dir(&missing)?;
    }
    assert!(!discard.exists());
    a.legacy(Some(&journal), 1000, false)?;
    assert!(a.recover(&journal)?);
    assert_eq!(fs::read_link(discard)?, missing);
    assert!(registry::registered(&a.state, &journal)?.active);
    Ok(())
}

#[test]
fn unload_keeps_original_and_installed_backups_and_unrelated_user_data() -> Result<()> {
    let a = Account::new()?;
    let target = a.home.join("project/skill");
    fs::create_dir_all(target.join("empty"))?;
    fs::write(target.join("SKILL.md"), b"previous\r\n")?;
    fs::write(target.with_file_name("user notes"), b"independent")?;
    let old = a.install(&target, true)?.expect("original backup");
    let removal = a.uninstall(&target)?.expect("uninstall backup");
    assert!(!target.exists());
    assert_eq!(fs::read(old.join("SKILL.md"))?, b"previous\r\n");
    assert!(old.join("empty").is_dir());
    assert_eq!(fs::read(removal.join("SKILL.md"))?, b"bundled skill");
    assert_eq!(
        fs::read(target.with_file_name("user notes"))?,
        b"independent"
    );
    Ok(())
}

#[test]
fn uninstall_refuses_user_edits_inside_installed_tree() -> Result<()> {
    let a = Account::new()?;
    let target = a.home.join("skill");
    a.install(&target, false)?;
    fs::write(target.join("mine"), b"user edit")?;
    let error = a
        .uninstall(&target)
        .expect_err("modified tree must survive");
    assert!(
        error.to_string().contains("Installed tree changed"),
        "{error}"
    );
    assert_eq!(fs::read(target.join("mine"))?, b"user edit");
    assert_eq!(fs::read(target.join("SKILL.md"))?, b"bundled skill");
    Ok(())
}

#[test]
fn uninstall_refuses_unregistered_user_tree() -> Result<()> {
    let a = Account::new()?;
    let target = a.home.join("manual skill");
    fs::create_dir(&target)?;
    fs::write(target.join("SKILL.md"), b"manual install")?;
    assert!(a.uninstall(&target).is_err());
    assert_eq!(fs::read(target.join("SKILL.md"))?, b"manual install");
    Ok(())
}

#[test]
fn next_install_reconciles_unfinished_committed_unregister() -> Result<()> {
    let a = Account::new()?;
    let (journal, _, target) = a.restored(false)?;
    let mut data = files::read_json(&journal)?;
    data["state"] = json!("committed");
    files::write_json(&journal, &data)?;
    registry::register(&a.state, &journal)?;
    assert!(registry::registered(&a.state, &journal)?.active);
    fs::remove_dir(target.parent().expect("live parent"))?;
    a.install(&a.home.join("next"), false)?;
    assert!(!registry::registered(&a.state, &journal)?.active);
    assert!(journal.is_file());
    Ok(())
}
