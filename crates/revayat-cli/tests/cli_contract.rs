use anyhow::Result;
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    process::{Command, Output, Stdio},
};

fn command(scratch: &tempfile::TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_revayat-subtitle"));
    command
        .current_dir(scratch.path())
        .env("REVAYAT_LOG_DIR", scratch.path())
        .env_remove("REVAYAT_LOG_LEVEL")
        .stdin(Stdio::null());
    command
}

fn assert_code(output: &Output, code: i32) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn help_lists_all_sixteen_commands_and_version() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let output = command(&scratch).arg("--help").output()?;
    assert_code(&output, 0);
    let help = String::from_utf8(output.stdout)?;
    for subcommand in [
        "doctor",
        "prepare",
        "build",
        "convert",
        "translator-ass",
        "sync",
        "render",
        "qa",
        "package",
        "status",
        "handoff",
        "supervise",
        "install",
        "lint",
        "lookup",
        "score",
    ] {
        assert!(
            help.lines()
                .any(|line| line.split_whitespace().next() == Some(subcommand)),
            "Missing {subcommand}"
        );
    }
    let output = command(&scratch).arg("--version").output()?;
    assert_code(&output, 0);
    assert_eq!(
        String::from_utf8(output.stdout)?.trim(),
        "revayat-subtitle 3.0.0"
    );
    Ok(())
}

#[test]
fn prepare_rejects_overflow_before_creating_a_workspace() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    for season in ["-1", "0", "100", "18446744073709551617"] {
        let output = command(&scratch)
            .args([
                "prepare", "clip.srt", "--work", "work", "--series", "Demo", "--season", season,
            ])
            .output()?;
        assert_code(&output, 2);
        let error = String::from_utf8(output.stderr)?;
        assert!(
            error.contains("--season") && error.contains("1..=99"),
            "{error}"
        );
        assert!(!scratch.path().join("work").exists());
    }
    Ok(())
}

#[test]
fn status_distinguishes_pending_work_from_a_stale_source() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    fs::write(
        scratch.path().join("clip.srt"),
        "1\n00:00:01,000 --> 00:00:02,000\nHello\n",
    )?;
    let output = command(&scratch)
        .args([
            "prepare", "clip.srt", "--work", "work", "--series", "Demo", "--season", "99",
        ])
        .output()?;
    assert_code(&output, 0);
    let args = ["status", "--work", "work"];
    let output = command(&scratch).args(args).output()?;
    assert_code(&output, 1);
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout)?["state"],
        "pending"
    );
    fs::write(scratch.path().join("work/sources/s0001.srt"), "changed")?;
    let output = command(&scratch).args(args).output()?;
    assert_code(&output, 2);
    assert!(String::from_utf8(output.stderr)?.starts_with("ERROR:"));
    Ok(())
}

#[test]
fn supervise_forwards_a_childs_nonstandard_exit_code() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    #[cfg(windows)]
    let child = ["cmd.exe", "/D", "/C", "exit", "7"];
    #[cfg(unix)]
    let child = ["sh", "-c", "exit 7"];
    let output = command(&scratch)
        .args(["supervise", "--timeout", "5", "--"])
        .args(child)
        .output()?;
    assert_code(&output, 7);
    Ok(())
}

#[test]
fn convert_preserves_json_and_refuses_to_replace_output() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    fs::write(
        scratch.path().join("sample clip.srt"),
        "1\n00:00:01,000 --> 00:00:02,000\nسلام\n",
    )?;
    let args = [
        "convert",
        "sample clip.srt",
        "--out",
        "converted clip.ass",
        "--log-level",
        "ERROR",
    ];
    let output = command(&scratch).args(args).output()?;
    assert_code(&output, 0);
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout)?,
        json!({"output":"converted clip.ass", "cues":1, "watermarks_removed":0})
    );
    let path = scratch.path().join("converted clip.ass");
    let original = fs::read(&path)?;
    let output = command(&scratch).args(args).output()?;
    assert_code(&output, 2);
    assert_eq!(fs::read(path)?, original);
    Ok(())
}

#[test]
fn lookup_reads_stdin_with_a_custom_bank() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    fs::create_dir(scratch.path().join("word bank"))?;
    fs::write(
        scratch.path().join("word bank/idioms.tsv"),
        "star sailor\tدریانورد\tExample\n",
    )?;
    let mut child = command(&scratch)
        .args(["lookup", "--json", "--data-dir", "word bank"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("No stdin pipe"))?
        .write_all(b"I met a star sailor.")?;
    let output = child.wait_with_output()?;
    assert_code(&output, 0);
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout)?,
        json!({"idioms":[{"en":"star sailor","fa":"دریانورد","note":"Example"}],"glossary":[]})
    );
    Ok(())
}

#[test]
fn fix_can_normalize_utf8_in_place() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("Persian text.txt");
    fs::write(&path, "كي\r\n")?;
    let args = ["lint", "--json", "Persian text.txt"];
    let output = command(&scratch).args(args).output()?;
    assert_code(&output, 1);
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout)?["changes"][0]["after"],
        "کی\r"
    );
    assert_code(
        &command(&scratch)
            .args(["lint", "--fix", "--in-place", "Persian text.txt"])
            .output()?,
        0,
    );
    assert_eq!(fs::read_to_string(path)?, "کی\r\n");
    assert_code(&command(&scratch).args(args).output()?, 0);
    Ok(())
}
