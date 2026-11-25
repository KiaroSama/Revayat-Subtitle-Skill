use revayat_core::{
    status,
    workflow::{
        import::{ImportOptions, prepare},
        workspace,
    },
};
use std::{fs, path::PathBuf};

fn pending_workspace() -> Result<(tempfile::TempDir, PathBuf), String> {
    let root = tempfile::tempdir().map_err(|error| error.to_string())?;
    let source = root.path().join("authored.srt");
    fs::write(&source, "1\n00:00:01,000 --> 00:00:02,000\nAuthored\n")
        .map_err(|error| error.to_string())?;
    let work = root.path().join("work");
    prepare(
        &[source],
        &work,
        &ImportOptions {
            series: "Status staging fixture",
            season: 1,
            encoding: "utf-8",
            glossary: None,
            language: "en",
            output_format: "srt",
        },
    )?;
    fs::create_dir(work.join("builds")).map_err(|error| error.to_string())?;
    Ok((root, work))
}

#[test]
fn active_and_interrupted_build_stages_do_not_poison_edition_inventory() -> Result<(), String> {
    let (_root, work) = pending_workspace()?;
    let before = status::report(&work)?;
    let stage = workspace::stage(&work.join("builds"), ".build-")?;
    fs::write(stage.path().join("unfinished"), b"keep staged bytes")
        .map_err(|error| error.to_string())?;
    let active = status::report(&work)?;
    assert_eq!(active["editions"], before["editions"]);
    assert_eq!(active["state"], "pending");
    assert_eq!(active["build_staging"], 1);
    let abandoned = stage.keep();
    let after_interrupt = status::report(&work)?;
    assert_eq!(after_interrupt, active);
    assert_eq!(
        fs::read(abandoned.join("unfinished")).map_err(|error| error.to_string())?,
        b"keep staged bytes"
    );
    fs::remove_dir_all(abandoned).map_err(|error| error.to_string())?;
    assert_eq!(status::report(&work)?, before);
    Ok(())
}

#[test]
fn arbitrary_entries_and_stage_named_files_still_invalidate_inventory() -> Result<(), String> {
    let (_root, work) = pending_workspace()?;
    for name in ["unknown", ".build-", ".build-short", ".build-1234567"] {
        let directory = work.join("builds").join(name);
        fs::create_dir(&directory).map_err(|error| error.to_string())?;
        assert!(status::report(&work).is_err());
        fs::remove_dir(directory).map_err(|error| error.to_string())?;
    }
    let file = work.join("builds/.build-ABC123");
    fs::write(&file, b"not a directory").map_err(|error| error.to_string())?;
    assert!(status::report(&work).is_err());
    fs::remove_file(file).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        let link = work.join("builds/.build-ABC123");
        std::os::unix::fs::symlink(&work, &link).map_err(|error| error.to_string())?;
        assert!(status::report(&work).is_err());
    }
    Ok(())
}

#[test]
fn unpublished_stages_count_toward_the_inventory_limit() -> Result<(), String> {
    let (_root, work) = pending_workspace()?;
    for index in 0..=1000 {
        fs::create_dir(work.join(format!("builds/.build-{index:06}")))
            .map_err(|error| error.to_string())?;
    }
    assert!(status::report(&work).is_err_and(|error| error.contains("limit")));
    Ok(())
}
