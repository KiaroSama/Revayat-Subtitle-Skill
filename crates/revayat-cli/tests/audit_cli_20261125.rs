//! Public CLI, diagnostic-sink, and strict JSON boundary regressions.

#![forbid(unsafe_code)]

use anyhow::Result;
use revayat_core::runtime::files;
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_revayat-subtitle"));
    command
        .current_dir(root)
        .env("REVAYAT_LOG_DIR", root.join("logs"))
        .env_remove("REVAYAT_LOG_LEVEL")
        .stdin(Stdio::null());
    command
}

fn log(root: &Path) -> Result<String> {
    let mut text = String::new();
    for entry in fs::read_dir(root.join("logs"))? {
        text.push_str(&fs::read_to_string(entry?.path())?);
    }
    Ok(text)
}

#[test]
fn runtime_failures_record_the_redacted_cause_and_final_status() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let output = command(temporary.path())
        .args(["status", "--work", "absent-work"])
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8(output.stderr)?.starts_with("ERROR:"));
    let log = log(temporary.path())?;
    assert!(
        log.contains("[ERROR] [revayat-subtitle] Command failed:"),
        "{log}"
    );
    assert!(log.contains("FileNotFoundError"), "{log}");
    assert!(log.contains("Finished exit_code=2"), "{log}");
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn parser_and_runtime_failures_do_not_panic_when_stderr_is_full() -> Result<()> {
    for args in [
        vec![
            "prepare",
            "input.srt",
            "--work",
            "work",
            "--series",
            "Demo",
            "--season",
            "0",
        ],
        vec!["status", "--work", "absent-work"],
    ] {
        let temporary = tempfile::tempdir()?;
        let output = command(temporary.path())
            .args(&args)
            .stderr(fs::OpenOptions::new().write(true).open("/dev/full")?)
            .output()?;
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        if args[0] == "status" {
            let log = log(temporary.path())?;
            assert!(log.contains("Command failed:"), "{log}");
            assert!(log.contains("Finished exit_code=2"), "{log}");
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn supervise_checks_buffered_stdout_flush_and_keeps_timeout_status() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let args = [
        "supervise",
        "--timeout",
        "5",
        "--",
        "/bin/printf",
        "authored short output",
    ];
    let output = command(temporary.path()).args(args).output()?;
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"authored short output");
    let output = command(temporary.path())
        .args(args)
        .stdout(fs::OpenOptions::new().write(true).open("/dev/full")?)
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    let output = command(temporary.path())
        .args(["supervise", "--timeout", "0.05", "--", "/bin/sleep", "5"])
        .stderr(fs::OpenOptions::new().write(true).open("/dev/full")?)
        .output()?;
    assert_eq!(output.status.code(), Some(124));
    Ok(())
}

#[cfg(unix)]
#[test]
fn supervise_maps_unix_signals_to_shell_status_and_error_log() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let output = command(temporary.path())
        .args([
            "supervise",
            "--timeout",
            "5",
            "--",
            "sh",
            "-c",
            "kill -TERM $$",
        ])
        .output()?;
    assert_eq!(output.status.code(), Some(143));
    assert!(log(temporary.path())?.contains("[ERROR] [revayat-subtitle] Finished exit_code=143"));
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn lint_batch_completes_all_fixes_when_advisory_stderr_is_full() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    for name in ["first.txt", "second.txt"] {
        fs::write(temporary.path().join(name), "كي")?;
    }
    let output = command(temporary.path())
        .args(["lint", "--fix", "--in-place", "first.txt", "second.txt"])
        .stderr(fs::OpenOptions::new().write(true).open("/dev/full")?)
        .output()?;
    assert_eq!(output.status.code(), Some(0));
    for name in ["first.txt", "second.txt"] {
        assert_eq!(fs::read_to_string(temporary.path().join(name))?, "کی");
    }
    assert!(log(temporary.path())?.contains("Finished exit_code=0"));
    Ok(())
}

#[cfg(unix)]
#[test]
fn lint_plain_filename_headings_cannot_inject_terminal_controls() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let hostile = "name\u{1b}[2J\nforged.txt";
    for name in [hostile, "normal.txt"] {
        fs::write(temporary.path().join(name), "clean")?;
    }
    let output = command(temporary.path())
        .args(["lint", hostile, "normal.txt"])
        .output()?;
    assert_eq!(output.status.code(), Some(0));
    let text = String::from_utf8(output.stdout)?;
    assert!(!text.contains('\u{1b}'));
    assert!(!text.contains("\nforged.txt:"));
    assert!(text.contains("normal.txt:"));
    Ok(())
}

#[test]
fn lookup_treats_a_non_directory_path_component_as_query_text() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    fs::write(temporary.path().join("N"), b"authored ordinary file")?;
    fs::create_dir(temporary.path().join("bank"))?;
    fs::write(
        temporary.path().join("bank/idioms.tsv"),
        "N/A\tAuthored translation\tAuthored note\n",
    )?;
    let output = command(temporary.path())
        .args(["lookup", "--json", "--data-dir", "bank", "N/A"])
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout)?["idioms"][0]["en"],
        "N/A"
    );
    Ok(())
}

#[test]
fn automatic_lookup_skips_empty_banks_but_explicit_empty_banks_stay_empty() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let portable = temporary.path().join("portable/bin");
    let empty = portable.join("voice/persian/scripts/data");
    let later = temporary.path().join("portable/voice/persian/data");
    fs::create_dir_all(&empty)?;
    fs::create_dir_all(&later)?;
    fs::write(
        later.join("idioms.tsv"),
        "star sailor\tAuthored translation\tAuthored note\n",
    )?;
    let executable = portable.join(format!("revayat-subtitle{}", std::env::consts::EXE_SUFFIX));
    fs::copy(env!("CARGO_BIN_EXE_revayat-subtitle"), &executable)?;
    let run = |explicit: bool| -> Result<Value> {
        let mut child = Command::new(&executable);
        child
            .current_dir(temporary.path())
            .env("REVAYAT_LOG_DIR", temporary.path().join("logs"))
            .env_remove("REVAYAT_LOG_LEVEL")
            .args(["lookup", "--json", "star sailor"]);
        if explicit {
            child.arg("--data-dir").arg(&empty);
        }
        // A sibling fork can briefly inherit the copied executable's write descriptor.
        let mut retries = 20;
        let output = loop {
            match child.output() {
                Err(err) if err.kind() == std::io::ErrorKind::ExecutableFileBusy && retries > 0 => {
                    retries -= 1;
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                result => break result?,
            }
        };
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    assert_eq!(run(false)?["idioms"][0]["en"], "star sailor");
    assert_eq!(run(true)?, json!({"idioms":[],"glossary":[]}));
    Ok(())
}

#[test]
fn json_integer_tokens_never_silently_round_into_floating_point() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("authored.json");
    for token in [
        "18446744073709551616",
        "18446744073709551617",
        "-9223372036854775809",
        "999999999999999999999",
    ] {
        for payload in [token.to_owned(), format!("{{\"integer\":{token}}}")] {
            fs::write(&path, &payload)?;
            assert!(
                files::read_json(&path, 4096).is_err(),
                "accepted integer {payload}"
            );
        }
    }
    for (token, expected) in [
        ("18446744073709551615", json!(u64::MAX)),
        ("-9223372036854775808", json!(i64::MIN)),
    ] {
        fs::write(&path, token)?;
        assert_eq!(
            files::read_json(&path, 4096).map_err(anyhow::Error::msg)?,
            expected
        );
    }
    for token in [
        "18446744073709551617.0",
        "1.8446744073709551617e19",
        "-9.223372036854776e18",
    ] {
        fs::write(&path, token)?;
        assert!(
            files::read_json(&path, 4096)
                .map_err(anyhow::Error::msg)?
                .is_f64()
        );
    }
    let quoted = json!({"quoted":"18446744073709551617 \" -9223372036854775809"});
    fs::write(&path, serde_json::to_vec(&quoted)?)?;
    assert_eq!(
        files::read_json(&path, 4096).map_err(anyhow::Error::msg)?,
        quoted
    );
    Ok(())
}
