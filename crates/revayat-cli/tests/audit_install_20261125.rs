//! Installer discovery, execute-bit ownership, and bounded distribution regressions.

use anyhow::Result;
use revayat_cli::install::{InstallArgs, distribution, files, install};
#[cfg(unix)]
use revayat_cli::install::{recovery, registry};
use serde_json::{Value, json};
#[cfg(unix)]
use std::path::Path;
use std::{fs, path::PathBuf};

struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    source: PathBuf,
}

impl Fixture {
    fn new(binary: bool) -> Result<Self> {
        let temporary = tempfile::tempdir()?;
        let root = files::safe_path(temporary.path(), true)?;
        let home = root.join("home");
        let source = root.join("source");
        fs::create_dir(&home)?;
        for name in ["revayat-subtitle", "revayat-subtitle-sync"] {
            let skill = source.join("skills").join(name);
            fs::create_dir_all(&skill)?;
            fs::write(skill.join("SKILL.md"), name)?;
        }
        if binary {
            fs::create_dir(source.join("bin"))?;
            fs::write(
                source.join("bin/revayat-subtitle"),
                b"authored executable bytes",
            )?;
        }
        Ok(Self {
            _temporary: temporary,
            root,
            home,
            source,
        })
    }

    fn args(&self, agent: &str, uninstall: bool) -> InstallArgs {
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
            uninstall,
        }
    }

    fn custom(&self, uninstall: bool) -> InstallArgs {
        let mut args = self.args("all", uninstall);
        args.destination = Some(self.root.join("target"));
        args.recovery_dir = Some(self.root.join("recovery"));
        args
    }

    fn run(&self, args: InstallArgs) -> Result<Value> {
        Ok(install(&self.source, &self.home, args)?)
    }

    #[cfg(unix)]
    fn journal(&self) -> Result<PathBuf> {
        Ok(fs::read_dir(self.root.join("recovery"))?
            .next()
            .expect("transaction")?
            .path()
            .join("journal.json"))
    }
}

#[test]
fn default_uninstall_skips_empty_hosts_and_handles_a_remaining_companion() -> Result<()> {
    for only_companion in [false, true] {
        let fixture = Fixture::new(false)?;
        fs::create_dir(fixture.home.join(".claude"))?;
        fs::create_dir(fixture.home.join(".cursor"))?;
        fixture.run(fixture.args("claude", false))?;
        let main = fixture.home.join(".claude/skills/revayat-subtitle");
        let sync = main.with_file_name("revayat-subtitle-sync");
        if only_companion {
            fs::remove_dir_all(&main)?;
        }
        let report = fixture.run(fixture.args("all", true))?;
        assert_eq!(report["state"], "uninstalled");
        assert_eq!(
            report["changes"].as_array().expect("changes").len(),
            if only_companion { 1 } else { 2 }
        );
        assert!(!main.exists() && !sync.exists());
        assert!(
            report["skipped"]
                .as_array()
                .expect("skipped")
                .contains(&json!("cursor"))
        );
    }
    Ok(())
}

#[test]
fn default_uninstall_preserves_foreign_named_skills_before_any_removal() -> Result<()> {
    let fixture = Fixture::new(false)?;
    fixture.run(fixture.args("claude", false))?;
    let foreign = fixture.home.join(".cursor/skills/revayat-subtitle");
    fs::create_dir_all(&foreign)?;
    fs::write(foreign.join("keep"), b"foreign bytes")?;
    assert!(fixture.run(fixture.args("all", true)).is_err());
    assert_eq!(fs::read(foreign.join("keep"))?, b"foreign bytes");
    assert!(
        fixture
            .home
            .join(".claude/skills/revayat-subtitle/SKILL.md")
            .is_file()
    );
    assert!(
        fixture
            .home
            .join(".claude/skills/revayat-subtitle-sync/SKILL.md")
            .is_file()
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn default_uninstall_refuses_broken_links_before_mutating_owned_hosts() -> Result<()> {
    let fixture = Fixture::new(false)?;
    fixture.run(fixture.args("claude", false))?;
    let foreign = fixture.home.join(".cursor/skills/revayat-subtitle");
    fs::create_dir_all(foreign.parent().expect("parent"))?;
    std::os::unix::fs::symlink(fixture.root.join("absent"), &foreign)?;
    assert!(fixture.run(fixture.args("all", true)).is_err());
    assert!(foreign.symlink_metadata()?.file_type().is_symlink());
    for name in ["revayat-subtitle", "revayat-subtitle-sync"] {
        assert!(
            fixture
                .home
                .join(".claude/skills")
                .join(name)
                .join("SKILL.md")
                .is_file()
        );
    }
    Ok(())
}

#[test]
fn explicit_uninstall_keeps_missing_target_errors() -> Result<()> {
    let fixture = Fixture::new(false)?;
    fs::create_dir(fixture.home.join(".cursor"))?;
    assert!(fixture.run(fixture.args("cursor", true)).is_err());
    assert!(!fixture.home.join("revayat-install-state").exists());
    Ok(())
}

#[cfg(unix)]
fn mode(path: &Path, value: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(value))?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn changed_execute_bits_refuse_uninstall_for_new_and_legacy_manifests() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    for legacy in [false, true] {
        let fixture = Fixture::new(true)?;
        fixture.run(fixture.custom(false))?;
        let binary = fixture.root.join("target/bin/revayat-subtitle");
        assert_eq!(fs::metadata(&binary)?.permissions().mode() & 0o777, 0o755);
        let journal = fixture.journal()?;
        if legacy {
            let (owner, _lock) = files::lock_owner(&fixture.home)?;
            let mut data = registry::load_registered(&owner, &journal, &fixture.home)?;
            for record in data["records"].as_array_mut().expect("records") {
                for field in ["old", "new"] {
                    if let Some(manifest) = record[field].as_object_mut() {
                        manifest.remove("executables");
                    }
                }
            }
            registry::save_journal(&owner, &journal, &data)?;
        }
        mode(&binary, 0o644)?;
        assert!(
            fixture.run(fixture.custom(true)).is_err(),
            "legacy={legacy}"
        );
        assert!(binary.is_file());
        assert_eq!(fs::metadata(&binary)?.permissions().mode() & 0o777, 0o644);
        mode(&binary, 0o755)?;
        let removed = fixture.run(fixture.custom(true))?;
        assert_eq!(removed["state"], "uninstalled");
        let backup = Path::new(removed["changes"][0]["backup"].as_str().expect("backup"));
        assert_eq!(
            fs::metadata(backup.join("bin/revayat-subtitle"))?
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn recovery_refuses_changed_old_execute_bits_and_preserves_original_mode() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new(false)?;
    let target = fixture.root.join("target");
    fs::create_dir(&target)?;
    fs::write(target.join("owned-tool"), b"authored old tool")?;
    mode(&target.join("owned-tool"), 0o700)?;
    let mut args = fixture.custom(false);
    args.force = true;
    fixture.run(args)?;
    let journal = fixture.journal()?;
    let (owner, _lock) = files::lock_owner(&fixture.home)?;
    let mut data = registry::load_registered(&owner, &journal, &fixture.home)?;
    data["state"] = json!("staged");
    data["records"][0]["phase"] = json!("published");
    registry::save_journal(&owner, &journal, &data)?;
    let backup = Path::new(data["records"][0]["backup"].as_str().expect("backup"));
    mode(&backup.join("owned-tool"), 0o600)?;
    assert!(recovery::restore(&journal, &owner, &fixture.home, true).is_err());
    assert!(target.join("SKILL.md").is_file());
    mode(&backup.join("owned-tool"), 0o700)?;
    assert!(recovery::restore(&journal, &owner, &fixture.home, true)?);
    assert_eq!(
        fs::metadata(target.join("owned-tool"))?
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn legacy_authenticated_recovery_retains_unchanged_old_tree_modes() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new(false)?;
    let target = fixture.root.join("target");
    fs::create_dir(&target)?;
    fs::write(target.join("old-tool"), b"authored legacy bytes")?;
    mode(&target.join("old-tool"), 0o700)?;
    let mut args = fixture.custom(false);
    args.force = true;
    fixture.run(args)?;
    let journal = fixture.journal()?;
    let (owner, _lock) = files::lock_owner(&fixture.home)?;
    let mut data = registry::load_registered(&owner, &journal, &fixture.home)?;
    data["state"] = json!("staged");
    data["records"][0]["phase"] = json!("published");
    for field in ["old", "new"] {
        data["records"][0][field]
            .as_object_mut()
            .expect("manifest")
            .remove("executables");
    }
    registry::save_journal(&owner, &journal, &data)?;
    assert!(recovery::restore(&journal, &owner, &fixture.home, true)?);
    assert_eq!(fs::read(target.join("old-tool"))?, b"authored legacy bytes");
    assert_eq!(
        fs::metadata(target.join("old-tool"))?.permissions().mode() & 0o777,
        0o700
    );
    assert!(recovery::restore(&journal, &owner, &fixture.home, true)?);
    Ok(())
}

#[test]
fn execute_bit_manifests_accept_legacy_and_reject_invalid_extensions() -> Result<()> {
    let legacy = json!({"files":{"tool":[0,"0".repeat(64)]},"directories":[]});
    files::validate_manifest(&legacy)?;
    for modes in [json!({}), json!({"tool":0o100}), json!({"tool":0o111})] {
        let mut manifest = legacy.clone();
        manifest["executables"] = modes;
        files::validate_manifest(&manifest)?;
    }
    for modes in [
        json!(null),
        json!([]),
        json!({"unknown":0o111}),
        json!({"tool":0}),
        json!({"tool":0o644}),
        json!({"tool":-1}),
        json!({"tool":1.5}),
        json!({"tool":"111"}),
    ] {
        let mut manifest = legacy.clone();
        manifest["executables"] = modes;
        assert!(files::validate_manifest(&manifest).is_err(), "{manifest}");
    }
    let mut manifest = legacy;
    manifest["unrecognized"] = json!({});
    assert!(files::validate_manifest(&manifest).is_err());
    Ok(())
}

#[test]
fn distribution_budget_stops_before_reading_later_files() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let mut payload = Vec::new();
    for index in 0..17 {
        let path = temporary.path().join(format!("part-{index}"));
        fs::File::create(&path)?.set_len(files::MAX_FILE)?;
        payload.push((path, PathBuf::from(format!("part-{index}"))));
    }
    payload.push((
        temporary.path().join("never-created"),
        PathBuf::from("later"),
    ));
    let error = distribution::distribution_manifest(&payload).expect_err("aggregate byte cap");
    assert!(error.to_string().contains("byte limit"), "{error}");
    Ok(())
}

#[test]
fn distribution_entry_budget_stops_before_opening_the_excess_file() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let source = temporary.path().join("empty");
    fs::write(&source, [])?;
    let mut payload: Vec<_> = (0..10_000)
        .map(|index| (source.clone(), PathBuf::from(format!("part-{index}"))))
        .collect();
    payload.push((
        temporary.path().join("never-created"),
        PathBuf::from("later"),
    ));
    let error = distribution::distribution_manifest(&payload).expect_err("aggregate entry cap");
    assert!(error.to_string().contains("entry limit"), "{error}");
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_recovery_discovery_components_are_case_insensitive() -> Result<()> {
    let fixture = Fixture::new(false)?;
    for name in ["Skills", "SKILLS", "Plugins", "PLUGINS"] {
        let mut args = fixture.custom(false);
        args.recovery_dir = Some(fixture.home.join(name).join("holding"));
        assert!(
            fixture.run(args).is_err(),
            "accepted discovery alias {name}"
        );
        assert!(!fixture.home.join(name).exists());
    }
    Ok(())
}
