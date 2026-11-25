use anyhow::{Result, ensure};
use revayat_cli::install::{
    InstallArgs, distribution, files, install, journal, recovery, registry, transaction,
};
use serde_json::{Value, json};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

struct Home {
    _scratch: tempfile::TempDir,
    home: PathBuf,
    source: PathBuf,
    holding: PathBuf,
}

impl Home {
    fn new() -> Result<Self> {
        let scratch = tempfile::tempdir()?;
        let root = files::safe_path(scratch.path(), true)?;
        let home = root.join("account home");
        let source = root.join("checkout with spaces");
        let holding = root.join("recovery");
        fs::create_dir(&home)?;
        for name in ["revayat-subtitle", "revayat-subtitle-sync"] {
            let skill = source.join("skills").join(name);
            fs::create_dir_all(&skill)?;
            fs::write(skill.join("SKILL.md"), name)?;
        }
        Ok(Self {
            _scratch: scratch,
            home,
            source,
            holding,
        })
    }

    fn args(&self, agent: &str) -> InstallArgs {
        InstallArgs {
            agent: agent.into(),
            scope: "user".into(),
            path: self.home.clone(),
            source: None,
            destination: None,
            plugin: false,
            force: false,
            dry_run: false,
            recovery_dir: None,
            recover: None,
            uninstall: false,
        }
    }

    fn run(&self, args: InstallArgs) -> Result<Value> {
        Ok(install(&self.source, &self.home, args)?)
    }

    fn plan(&self, targets: &[PathBuf], force: bool) -> io::Result<transaction::Plan> {
        transaction::plan_install(
            &self.source,
            targets,
            false,
            force,
            Some(&self.holding),
            false,
        )
    }
}

fn installs_agent(agent: &str, folder: &str) -> Result<()> {
    let h = Home::new()?;
    let output = h.run(h.args(agent))?;
    assert_eq!(output["state"], "installed");
    assert_eq!(output["changes"].as_array().map(Vec::len), Some(2));
    for skill in ["revayat-subtitle", "revayat-subtitle-sync"] {
        assert_eq!(
            fs::read_to_string(
                h.home
                    .join(folder)
                    .join("skills")
                    .join(skill)
                    .join("SKILL.md")
            )?,
            skill
        );
    }
    Ok(())
}

macro_rules! agents {
    ($($test:ident: $agent:literal => $folder:literal),+ $(,)?) => {$ (
        #[test]
        fn $test() -> Result<()> { installs_agent($agent, $folder) }
    )+};
}

agents! {
    claude: "claude" => ".claude", codex: "codex" => ".agents", cursor: "cursor" => ".cursor",
    kiro: "kiro" => ".kiro", cline: "cline" => ".cline", hermes: "hermes" => ".hermes",
    opencode: "opencode" => ".config/opencode", antigravity: "antigravity" => ".gemini/antigravity",
    antigravity_cli: "antigravity-cli" => ".gemini/antigravity-cli",
}

#[test]
fn all_installs_all_nine_existing_user_hosts() -> Result<()> {
    let h = Home::new()?;
    for agent in distribution::AGENTS {
        let target = distribution::destination(agent, "user", &h.home).expect("known host");
        fs::create_dir_all(target.ancestors().nth(2).expect("agent root"))?;
    }
    let report = h.run(h.args("all"))?;
    assert_eq!(report["changes"].as_array().map(Vec::len), Some(18));
    assert_eq!(report["skipped"], json!([]));
    Ok(())
}

#[test]
fn all_skips_absent_user_hosts() -> Result<()> {
    let h = Home::new()?;
    fs::create_dir(h.home.join(".claude"))?;
    let report = h.run(h.args("all"))?;
    assert_eq!(report["changes"].as_array().map(Vec::len), Some(2));
    assert_eq!(report["skipped"].as_array().map(Vec::len), Some(8));
    assert!(!h.home.join(".cursor").exists());
    Ok(())
}

#[test]
fn all_user_install_with_no_hosts_leaves_account_empty() -> Result<()> {
    let h = Home::new()?;
    assert!(h.run(h.args("all")).is_err());
    assert_eq!(fs::read_dir(&h.home)?.count(), 0);
    Ok(())
}

#[test]
fn project_all_deduplicates_shared_agent_discovery() -> Result<()> {
    let h = Home::new()?;
    let mut args = h.args("all");
    args.scope = "project".into();
    let report = h.run(args)?;
    let items = report["changes"].as_array().expect("changes");
    assert_eq!(items.len(), 14);
    assert_eq!(
        items
            .iter()
            .filter(|item| item["target"]
                .as_str()
                .is_some_and(|p| Path::new(p).ends_with(".agents/skills/revayat-subtitle")))
            .count(),
        1
    );
    Ok(())
}

#[test]
fn dry_run_has_no_filesystem_effect() -> Result<()> {
    let h = Home::new()?;
    let mut args = h.args("claude");
    args.dry_run = true;
    let report = h.run(args)?;
    assert_eq!(report["state"], "dry-run");
    assert_eq!(report["targets"].as_array().map(Vec::len), Some(2));
    assert_eq!(fs::read_dir(&h.home)?.count(), 0);
    Ok(())
}

#[test]
fn custom_target_requires_recovery() -> Result<()> {
    let h = Home::new()?;
    let mut args = h.args("claude");
    args.destination = Some(h.home.join("custom"));
    assert!(
        h.run(args).is_err(),
        "install unexpectedly accepted no recovery root"
    );
    assert!(!h.home.join("custom").exists());
    Ok(())
}

#[test]
fn custom_target_ignores_unused_project_path() -> Result<()> {
    let h = Home::new()?;
    let mut args = h.args("claude");
    args.scope = "project".into();
    args.path = h.home.join("not a project");
    let target = h.home.join("custom");
    args.destination = Some(target.clone());
    args.recovery_dir = Some(h.holding.clone());
    assert_eq!(h.run(args)?["state"], "installed");
    assert!(target.join("SKILL.md").is_file());
    Ok(())
}

#[test]
fn recovery_under_skills_is_refused_before_discovery_changes() -> Result<()> {
    let h = Home::new()?;
    let mut args = h.args("claude");
    args.destination = Some(h.home.join("custom"));
    args.recovery_dir = Some(h.home.join("skills/backup"));
    args.dry_run = true;
    assert!(h.run(args).is_err());
    assert_eq!(fs::read_dir(&h.home)?.count(), 0);
    Ok(())
}

#[test]
fn distribution_destination_overlap_is_refused() -> Result<()> {
    let h = Home::new()?;
    let target = h.source.join("skills/revayat-subtitle");
    assert!(h.plan(std::slice::from_ref(&target), true).is_err());
    assert_eq!(
        fs::read_to_string(target.join("SKILL.md"))?,
        "revayat-subtitle"
    );
    Ok(())
}

#[test]
fn replaced_agent_ancestor_is_not_adopted() -> Result<()> {
    let h = Home::new()?;
    let parent = h.home.join("new-host");
    let target = parent.join("skill");
    let plan = h.plan(std::slice::from_ref(&target), false)?;
    fs::create_dir(&parent)?;
    fs::write(parent.join("user-owned"), b"untouched")?;
    assert!(transaction::execute_plan(plan, &h.home).is_err());
    assert_eq!(fs::read(parent.join("user-owned"))?, b"untouched");
    assert!(!target.exists());
    Ok(())
}

#[test]
fn unsafe_late_target_keeps_earlier_host_uninstalled() -> Result<()> {
    let h = Home::new()?;
    let early = h.home.join("early");
    let late = h.home.join("late");
    fs::write(&late, b"user file")?;
    assert!(h.plan(&[early.clone(), late.clone()], true).is_err());
    assert!(!early.exists());
    assert_eq!(fs::read(late)?, b"user file");
    Ok(())
}

#[test]
fn stdlib_lock_contention_fails_fast_and_does_not_unlink() -> Result<()> {
    let h = Home::new()?;
    let (owner, initial) = files::lock_owner(&h.home)?;
    let lock_path = owner.join("owner.lock");
    let stable = files::identity(&lock_path)?;
    drop(initial);
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)?;
    lock.try_lock()?;
    let mut args = h.args("claude");
    args.recovery_dir = Some(h.holding.clone());
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let task = scope.spawn(|| send.send(h.run(args)));
        let attempted = receive.recv_timeout(Duration::from_secs(5));
        drop(lock);
        task.join()
            .expect("installation thread")
            .expect("test receiver");
        let error = attempted
            .expect("installation must fail without waiting for the lock")
            .expect_err("installer must not take the held lock");
        assert!(error.to_string().contains("holds the OS lock"), "{error}");
    });
    assert!(!h.home.join(".claude").exists());
    assert_eq!(h.run(h.args("claude"))?["state"], "installed");
    assert_eq!(files::identity(&lock_path)?, stable);
    Ok(())
}

#[test]
fn force_keeps_discoverable_backup_outside_active_skills() -> Result<()> {
    let h = Home::new()?;
    let target = h.home.join(".claude/skills/revayat-subtitle");
    fs::create_dir_all(target.join("empty"))?;
    fs::write(target.join("SKILL.md"), b"old custom skill\r\n")?;
    fs::write(target.join("custom.bin"), [0, 255, 1])?;
    let mut args = h.args("claude");
    args.force = true;
    let report = h.run(args)?;
    let backup = Path::new(report["changes"][0]["backup"].as_str().expect("old backup"));
    assert!(!backup.starts_with(target.parent().expect("skills parent")));
    assert_eq!(fs::read(backup.join("custom.bin"))?, [0, 255, 1]);
    assert_eq!(fs::read(backup.join("SKILL.md"))?, b"old custom skill\r\n");
    assert!(backup.join("empty").is_dir());
    assert_eq!(
        fs::read_to_string(target.join("SKILL.md"))?,
        "revayat-subtitle"
    );
    Ok(())
}

#[test]
fn no_force_preserves_user_tree_and_force_retries() -> Result<()> {
    let h = Home::new()?;
    let target = h.home.join(".claude/skills/revayat-subtitle");
    fs::create_dir_all(&target)?;
    fs::write(target.join("SKILL.md"), b"local edits")?;
    assert!(h.run(h.args("claude")).is_err());
    assert_eq!(fs::read(target.join("SKILL.md"))?, b"local edits");
    let mut force = h.args("claude");
    force.force = true;
    assert_eq!(h.run(force)?["state"], "installed");
    Ok(())
}

fn link_dir(target: &Path, link: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        let output = std::process::Command::new(
            std::env::var_os("COMSPEC").unwrap_or_else(|| "cmd.exe".into()),
        )
        .args(["/d", "/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .stdin(std::process::Stdio::null())
        .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(io::Error::other(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ))
        }
    }
}

#[test]
fn linked_destination_is_refused_without_touching_referent() -> Result<()> {
    let h = Home::new()?;
    let outside = h.home.join("referent");
    let link = h.home.join("linked");
    fs::create_dir(&outside)?;
    fs::write(outside.join("user"), b"keep")?;
    link_dir(&outside, &link)?;
    assert!(h.plan(&[link], true).is_err());
    assert_eq!(fs::read(outside.join("user"))?, b"keep");
    Ok(())
}

#[test]
fn linked_ancestor_of_missing_destination_is_refused() -> Result<()> {
    let h = Home::new()?;
    let outside = h.home.join("referent");
    let link = h.home.join("linked");
    fs::create_dir(&outside)?;
    link_dir(&outside, &link)?;
    assert!(h.plan(&[link.join("new")], false).is_err());
    assert_eq!(fs::read_dir(outside)?.count(), 0);
    Ok(())
}

struct Interrupted {
    _lock: fs::File,
    h: Home,
    owner: PathBuf,
    journal: PathBuf,
    target: PathBuf,
    stage: PathBuf,
    backup: PathBuf,
    disposal: PathBuf,
    data: Value,
}

impl Interrupted {
    fn new(published: bool) -> Result<Self> {
        let h = Home::new()?;
        let (owner, lock) = files::lock_owner(&h.home)?;
        let id = "0123456789abcdef0123456789abcdef";
        let tx = h.holding.join(format!("transaction-{id}"));
        let target = h.home.join("host").join("skill");
        let stage = tx.join("new-0000");
        let backup = tx.join("old-0000");
        let disposal = tx.join("dispose-0000");
        let journal = tx.join("journal.json");
        fs::create_dir_all(backup.join("empty"))?;
        fs::write(backup.join("SKILL.md"), b"old\r\n")?;
        let host = target.parent().expect("host parent");
        fs::create_dir(host)?;
        fs::create_dir(host.join("unrelated-empty"))?;
        let new = if published { &target } else { &stage };
        fs::create_dir(new)?;
        fs::write(new.join("SKILL.md"), b"new")?;
        let ancestors: serde_json::Map<String, Value> = tx
            .ancestors()
            .chain(host.ancestors())
            .map(|p| Ok((p.to_string_lossy().into_owned(), json!(files::identity(p)?))))
            .collect::<io::Result<_>>()?;
        let data = json!({"version":1,"id":id,"owner":files::safe_path(&h.home, true)?,"recovery":h.holding,"state":"staged",
            "records":[{"target":target,"stage":stage,"backup":backup,"disposal":disposal,
                "previous_id":files::identity(&backup)?,"stage_id":files::identity(new)?,
                "old":files::tree_manifest(&backup)?,"new":files::tree_manifest(new)?,
                "phase":if published {"published"} else {"old-moved"}}],"ancestors":ancestors});
        files::write_json(&journal, &data)?;
        registry::register(&owner, &journal)?;
        Ok(Self {
            h,
            owner,
            _lock: lock,
            journal,
            target,
            stage,
            backup,
            disposal,
            data,
        })
    }

    fn recover(&self) -> io::Result<bool> {
        recovery::recover_owned(&self.journal, &self.h.holding, &self.owner, &self.h.home)
    }

    fn old_is_restored(&self) -> Result<()> {
        assert_eq!(fs::read(self.target.join("SKILL.md"))?, b"old\r\n");
        assert!(self.target.join("empty").is_dir());
        assert!(self.target.with_file_name("unrelated-empty").is_dir());
        assert!(!self.backup.exists());
        assert!(!self.stage.exists());
        assert!(!self.disposal.exists());
        assert_eq!(
            journal::load_journal(&self.journal, &self.h.home)?["state"],
            "restored"
        );
        Ok(())
    }
}

#[test]
fn recovery_after_old_move_restores_bytes_and_empty_directories() -> Result<()> {
    let tx = Interrupted::new(false)?;
    assert!(!tx.target.exists());
    assert!(tx.recover()?);
    tx.old_is_restored()
}

#[test]
fn recovery_after_publication_restores_old_and_reclaims_new() -> Result<()> {
    let tx = Interrupted::new(true)?;
    assert_eq!(fs::read(tx.target.join("SKILL.md"))?, b"new");
    assert!(tx.recover()?);
    tx.old_is_restored()
}

#[test]
fn interrupted_quarantine_move_is_restartable() -> Result<()> {
    let tx = Interrupted::new(true)?;
    let interrupted =
        recovery::restore_with(&tx.journal, &tx.owner, &tx.h.home, true, |from, to| {
            files::move_owned(from, to)?;
            Err(io::Error::other("interrupted after rename"))
        });
    assert_eq!(
        interrupted.expect_err("failure injection").to_string(),
        "interrupted after rename"
    );
    assert!(!tx.target.exists());
    assert!(tx.disposal.is_dir());
    assert!(tx.recover()?);
    tx.old_is_restored()
}

#[test]
fn foreign_recovery_root_refuses_without_moving_trees() -> Result<()> {
    let tx = Interrupted::new(true)?;
    assert!(
        recovery::recover_owned(&tx.journal, &tx.h.home.join("other"), &tx.owner, &tx.h.home)
            .is_err()
    );
    assert_eq!(fs::read(tx.backup.join("SKILL.md"))?, b"old\r\n");
    assert_eq!(fs::read(tx.target.join("SKILL.md"))?, b"new");
    Ok(())
}

#[test]
fn valid_but_rewritten_journal_is_not_authoritative() -> Result<()> {
    let mut tx = Interrupted::new(true)?;
    tx.data["records"][0]["phase"] = json!("new-intent");
    files::write_json(&tx.journal, &tx.data)?;
    assert!(journal::load_journal(&tx.journal, &tx.h.home).is_ok());
    assert!(tx.recover().is_err());
    assert_eq!(fs::read(tx.target.join("SKILL.md"))?, b"new");
    assert_eq!(fs::read(tx.backup.join("SKILL.md"))?, b"old\r\n");
    Ok(())
}

#[test]
fn modified_backup_cannot_be_restored() -> Result<()> {
    let tx = Interrupted::new(true)?;
    fs::write(tx.backup.join("SKILL.md"), b"foreign")?;
    assert!(tx.recover().is_err());
    assert_eq!(fs::read(tx.target.join("SKILL.md"))?, b"new");
    assert_eq!(fs::read(tx.backup.join("SKILL.md"))?, b"foreign");
    Ok(())
}

#[test]
fn foreign_stage_identity_cannot_be_removed() -> Result<()> {
    let tx = Interrupted::new(false)?;
    fs::rename(&tx.stage, tx.stage.with_extension("owned"))?;
    fs::create_dir(&tx.stage)?;
    fs::write(tx.stage.join("SKILL.md"), b"foreign")?;
    let error = tx
        .recover()
        .expect_err("foreign stage must not be treated as owned");
    assert!(error.to_string().contains("staging identity"));
    assert!(!tx.target.exists());
    assert_eq!(fs::read(tx.stage.join("SKILL.md"))?, b"foreign");
    assert_eq!(fs::read(tx.backup.join("SKILL.md"))?, b"old\r\n");
    Ok(())
}

#[test]
fn stage_created_before_identity_was_bound_is_preserved() -> Result<()> {
    let mut tx = Interrupted::new(false)?;
    files::move_owned(&tx.backup, &tx.target)?;
    tx.data["state"] = json!("staging");
    tx.data["records"][0]["phase"] = json!("staging");
    tx.data["records"][0]["stage_id"] = Value::Null;
    registry::save_journal(&tx.owner, &tx.journal, &tx.data)?;
    let error = tx.recover().expect_err("stage without a bound identity");
    assert!(error.to_string().contains("staging identity"));
    assert_eq!(fs::read(tx.target.join("SKILL.md"))?, b"old\r\n");
    assert_eq!(fs::read(tx.stage.join("SKILL.md"))?, b"new");
    Ok(())
}

#[test]
fn malformed_or_unknown_journal_fields_are_refused_at_schema_boundary() -> Result<()> {
    let tx = Interrupted::new(true)?;
    for (field, wrong) in [
        ("version", json!(99)),
        ("state", json!([])),
        ("recovery", json!({})),
    ] {
        let mut data = tx.data.clone();
        data[field] = wrong;
        files::write_json(&tx.journal, &data)?;
        assert!(
            journal::load_journal(&tx.journal, &tx.h.home).is_err(),
            "accepted malformed {field}"
        );
    }
    fs::write(&tx.journal, b"{")?;
    assert!(journal::load_journal(&tx.journal, &tx.h.home).is_err());
    assert!(files::valid_identity(&json!(["1", "f".repeat(32)])));
    assert!(!files::valid_identity(&json!(["1", "f".repeat(33)])));
    assert_eq!(fs::read(tx.target.join("SKILL.md"))?, b"new");
    Ok(())
}

#[test]
fn replaced_live_ancestor_refuses_active_recovery() -> Result<()> {
    let tx = Interrupted::new(false)?;
    let host = tx.target.parent().expect("host parent");
    fs::rename(host, host.with_extension("original"))?;
    fs::create_dir(host)?;
    assert!(tx.recover().is_err());
    assert!(!tx.target.exists());
    assert_eq!(fs::read(tx.backup.join("SKILL.md"))?, b"old\r\n");
    Ok(())
}

#[test]
fn pending_interrupted_install_requires_explicit_recovery() -> Result<()> {
    let tx = Interrupted::new(false)?;
    let plan = tx.h.plan(std::slice::from_ref(&tx.target), false)?;
    let error = transaction::execute_owned(plan, &tx.owner, &tx.h.home)
        .expect_err("unfinished transaction must block overlap");
    ensure!(error.to_string().contains("explicit --recover"), "{error}");
    assert!(!tx.target.exists());
    Ok(())
}

#[test]
fn recovery_does_not_uninstall_a_committed_transaction() -> Result<()> {
    let mut tx = Interrupted::new(true)?;
    tx.data["state"] = json!("committed");
    registry::save_journal(&tx.owner, &tx.journal, &tx.data)?;
    let error = tx
        .recover()
        .expect_err("committed installs cannot be recovered");
    assert!(error.to_string().contains("does not authorize uninstall"));
    assert_eq!(fs::read(tx.target.join("SKILL.md"))?, b"new");
    assert_eq!(fs::read(tx.backup.join("SKILL.md"))?, b"old\r\n");
    Ok(())
}

#[test]
fn recovery_authorizes_only_the_two_digests_at_the_binding_boundary() -> Result<()> {
    let tx = Interrupted::new(false)?;
    let mut next = tx.data.clone();
    next["records"][0]["phase"] = json!("new-intent");
    let pending = tx.owner.join("pending.json");
    let mut signed = files::read_json(&pending)?;
    let current = files::digest(&fs::read(&tx.journal)?);
    let intended = files::digest(&files::json_bytes(&next)?);
    signed["records"][0]["digests"] = json!([current, intended]);
    use hmac::{Hmac, KeyInit, Mac};
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(&fs::read(tx.owner.join("registry.key"))?)?;
    mac.update(b"revayat-install-registry-v1\0");
    mac.update(&serde_json::to_vec(&signed["records"])?);
    signed["hmac"] = json!(hex::encode(mac.finalize().into_bytes()));
    files::write_json(&pending, &signed)?;
    assert_eq!(
        registry::load_registered(&tx.owner, &tx.journal, &tx.h.home)?,
        tx.data
    );
    next["records"][0]["phase"] = json!("staged");
    files::write_json(&tx.journal, &next)?;
    assert!(tx.recover().is_err());
    assert!(!tx.target.exists());
    next["records"][0]["phase"] = json!("new-intent");
    files::write_json(&tx.journal, &next)?;
    assert!(tx.recover()?);
    tx.old_is_restored()
}

#[test]
fn failed_second_restoration_leaves_the_old_backup_retryable() -> Result<()> {
    let tx = Interrupted::new(true)?;
    let error = recovery::restore_with(&tx.journal, &tx.owner, &tx.h.home, true, |from, to| {
        if from == tx.backup {
            Err(io::Error::other("old tree move blocked"))
        } else {
            files::move_owned(from, to)
        }
    })
    .expect_err("old tree move should fail");
    assert_eq!(error.to_string(), "old tree move blocked");
    assert!(!tx.target.exists());
    assert_eq!(fs::read(tx.backup.join("SKILL.md"))?, b"old\r\n");
    assert!(registry::registered(&tx.owner, &tx.journal)?.active);
    assert!(tx.recover()?);
    tx.old_is_restored()
}

#[test]
fn recovery_preflights_every_record_before_moving_any_live_tree() -> Result<()> {
    let mut tx = Interrupted::new(true)?;
    let mut second = tx.data["records"][0].clone();
    let parent = tx.journal.parent().expect("transaction directory");
    let target = tx.target.with_file_name("second skill");
    let backup = parent.join("old-0001");
    fs::create_dir(&target)?;
    fs::create_dir(&backup)?;
    fs::write(target.join("SKILL.md"), b"new second")?;
    fs::write(backup.join("SKILL.md"), b"old second")?;
    second["target"] = json!(target);
    second["stage"] = json!(parent.join("new-0001"));
    second["backup"] = json!(backup);
    second["disposal"] = json!(parent.join("dispose-0001"));
    second["previous_id"] = json!(files::identity(&backup)?);
    second["stage_id"] = json!(files::identity(&target)?);
    second["old"] = files::tree_manifest(&backup)?;
    second["new"] = files::tree_manifest(&target)?;
    tx.data["records"]
        .as_array_mut()
        .expect("journal records")
        .push(second);
    registry::save_journal(&tx.owner, &tx.journal, &tx.data)?;
    fs::write(tx.backup.join("SKILL.md"), b"changed before recovery")?;
    let error = tx
        .recover()
        .expect_err("preflight must reject the bad backup");
    assert!(error.to_string().contains("backup changed"));
    assert_eq!(fs::read(target.join("SKILL.md"))?, b"new second");
    assert_eq!(fs::read(backup.join("SKILL.md"))?, b"old second");
    assert!(!parent.join("dispose-0001").exists());
    Ok(())
}

#[test]
fn install_distribution_drops_legacy_executables() -> Result<()> {
    let h = Home::new()?;
    let skill = h.source.join("skills/revayat-subtitle");
    for name in ["legacy.py", "legacy.ps1", "legacy.sh", "legacy.PY"] {
        fs::write(skill.join(name), b"legacy")?;
    }
    fs::write(skill.join("terms.tsv"), b"term\tvalue\n")?;
    let manifest = distribution::payloads(&h.source, false, true)?;
    let names: Vec<_> = manifest["revayat-subtitle"]
        .iter()
        .map(|(_, path)| path.as_path())
        .collect();
    assert_eq!(names, [Path::new("SKILL.md"), Path::new("terms.tsv")]);
    Ok(())
}
