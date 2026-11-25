use serde_json::Value;
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn lint(text: &str, fix: bool) -> Result<Output, Box<dyn std::error::Error>> {
    let logs = tempfile::tempdir()?;
    let mut command = Command::new(env!("CARGO_BIN_EXE_revayat-subtitle"));
    command
        .env("REVAYAT_LOG_DIR", logs.path())
        .args(["lint", "--style", "--digits", "keep"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if fix {
        command.arg("--fix");
    } else {
        command.args(["--check", "--json"]);
    }
    let mut child = command.spawn()?;
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(text.as_bytes())?;
    Ok(child.wait_with_output()?)
}

#[test]
fn style_cli_runs_the_normalization_prepass_for_long_inputs()
-> Result<(), Box<dyn std::error::Error>> {
    let suffix = " در پایان";
    let original = format!("{}\nخوب ترین...", "x".repeat(200_000));
    for (text, style_count) in [
        (original.clone(), 0),
        (format!("یک {} است", "x".repeat(1_000_000)), 1),
        (
            format!("{}{suffix}", "x".repeat(16 * 1024 * 1024 - suffix.len())),
            1,
        ),
    ] {
        let output = lint(&text, false)?;
        assert_eq!(
            output.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(
            report["suggestions"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|hint| hint["type"] == "style")
                .count(),
            style_count
        );
    }
    let output = lint(&original, true)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout)?,
        original.replace("خوب ترین...", "خوب‌ترین…")
    );
    Ok(())
}
