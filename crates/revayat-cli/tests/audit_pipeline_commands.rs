//! Public command regressions using only authored subtitle fixtures.

use anyhow::Result;
use revayat_core::{runtime::files, subtitle_formats::document};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
};

const SOURCE: &str = "1\n00:00:00,000 --> 00:00:01,000\nAuthored dialogue\n";

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_revayat-subtitle"));
    command
        .current_dir(root)
        .env("REVAYAT_LOG_DIR", root.join("logs"))
        .env_remove("REVAYAT_LOG_LEVEL")
        .stdin(Stdio::null());
    command
}

fn status(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn prepare(root: &Path, work: &Path, encoding: &str) -> Result<Output> {
    Ok(command(root)
        .args(["prepare", "input.srt", "--work"])
        .arg(work)
        .args([
            "--series",
            "Authored",
            "--season",
            "1",
            "--target-language",
            "en",
            "--output-format",
            "srt",
            "--encoding",
            encoding,
        ])
        .output()?)
}

fn read(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn write(path: &Path, value: &Value) -> Result<()> {
    files::write_json(path, value).map_err(anyhow::Error::msg)
}

fn reviewed(root: &Path, text: &str) -> Result<()> {
    fs::write(
        root.join("input.srt"),
        SOURCE.replace("Authored dialogue", text),
    )?;
    status(&prepare(root, Path::new("work"), "utf-8")?, 0);
    let work = root.join("work");
    let mut project = read(&work.join("project.json"))?;
    project["episodes"] =
        json!([{"id": "S01E01", "base": "s0001", "comparison": "Authored fixture"}]);
    write(&work.join("project.json"), &project)?;
    let mut sheet = read(&work.join("worksheets/s0001.json"))?;
    for row in sheet
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("worksheet"))?
    {
        row["action"] = json!("preserve");
        row["reviewed"] = json!(true);
    }
    write(&work.join("worksheets/s0001.json"), &sheet)?;
    write(
        &work.join("glossary.json"),
        &json!({"series": "Authored", "terms": [], "terms_reviewed": true,
        "research": [{"url": "https://example.test", "note": "Authored fixture"}]}),
    )
}

#[test]
fn convert_accepts_extra_blank_lines_without_dropping_cues() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    fs::write(
        root.join("input.srt"),
        format!("{SOURCE}\n\n2\n00:00:02,000 --> 00:00:03,000\nSecond\n"),
    )?;
    let output = command(root)
        .args([
            "convert",
            "input.srt",
            "--out",
            "output.ass",
            "--direction",
            "ltr",
            "--no-zwnj",
        ])
        .output()?;
    status(&output, 0);
    let parsed = document::parse(&fs::read_to_string(root.join("output.ass"))?, "ass")
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        parsed
            .cues
            .iter()
            .map(|cue| cue.text.as_str())
            .collect::<Vec<_>>(),
        ["Authored dialogue", "Second"]
    );
    Ok(())
}

#[test]
fn build_refuses_new_hidden_comments_and_keeps_the_workspace_retryable() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    reviewed(root, "A<!<!--note-->--hidden-->B")?;
    let source = fs::read(root.join("work/sources/s0001.srt"))?;
    let output = command(root).args(["build", "--work", "work"]).output()?;
    status(&output, 2);
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8(output.stderr)?.contains("activate literal markup"));
    assert!(!root.join("work/builds").exists());
    assert_eq!(fs::read(root.join("work/sources/s0001.srt"))?, source);
    let sheet_path = root.join("work/worksheets/s0001.json");
    let mut sheet = read(&sheet_path)?;
    sheet[0]["action"] = json!("edit");
    sheet[0]["text"] = json!("Authored explicit adaptation");
    sheet[0]["structure_note"] = json!("Replace ambiguous comment delimiters after inspection");
    write(&sheet_path, &sheet)?;
    let output = command(root).args(["build", "--work", "work"]).output()?;
    status(&output, 0);
    assert_eq!(fs::read(root.join("work/sources/s0001.srt"))?, source);
    Ok(())
}

#[test]
fn marker_looking_metadata_changes_the_public_build_identity() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    reviewed(root, "Authored dialogue")?;
    let project_path = root.join("work/project.json");
    let mut project = read(&project_path)?;
    let mut identities = Vec::new();
    for metadata in [json!({"$tuple": [1]}), json!([1])] {
        project["user_metadata"] = metadata;
        write(&project_path, &project)?;
        let output = command(root).args(["build", "--work", "work"]).output()?;
        status(&output, 0);
        identities.push(serde_json::from_slice::<Value>(&output.stdout)?["identity"].clone());
    }
    assert_ne!(identities[0], identities[1]);
    assert_eq!(fs::read_dir(root.join("work/builds"))?.count(), 2);
    Ok(())
}

#[test]
fn prepare_decodes_utf32_and_latin1_aliases_with_exact_source_retention() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    let text = SOURCE.replace("Authored dialogue", "سلام 🙂");
    let raw = text
        .chars()
        .flat_map(|c| (c as u32).to_le_bytes())
        .collect::<Vec<_>>();
    fs::write(root.join("input.srt"), &raw)?;
    status(&prepare(root, Path::new("utf32"), "utf-32-le")?, 0);
    assert_eq!(
        read(&root.join("utf32/worksheets/s0001.json"))?[0]["source_text"],
        "سلام 🙂"
    );
    assert_eq!(fs::read(root.join("utf32/sources/s0001.srt"))?, raw);
    let mut raw = SOURCE.replace("Authored dialogue", "BYTE").into_bytes();
    let at = raw
        .windows(4)
        .position(|part| part == b"BYTE")
        .ok_or_else(|| anyhow::anyhow!("byte marker"))?;
    raw.splice(at..at + 4, [0x80]);
    fs::write(root.join("input.srt"), &raw)?;
    status(&prepare(root, Path::new("latin"), "iso8859-1")?, 0);
    assert_eq!(
        read(&root.join("latin/worksheets/s0001.json"))?[0]["source_text"],
        "\u{80}"
    );
    assert_eq!(fs::read(root.join("latin/sources/s0001.srt"))?, raw);
    Ok(())
}

#[test]
fn native_ass_conversion_preserves_nested_override_font_bytes() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    fs::write(root.join("input.srt"), SOURCE)?;
    status(
        &command(root)
            .args([
                "convert",
                "input.srt",
                "--out",
                "input.ass",
                "--direction",
                "ltr",
                "--no-zwnj",
            ])
            .output()?,
        0,
    );
    let native = fs::read_to_string(root.join("input.ass"))?
        .replace("Authored dialogue", r"{\fnمي روم{X}مي روم");
    fs::write(root.join("input.ass"), native)?;
    status(
        &command(root)
            .args([
                "convert",
                "input.ass",
                "--out",
                "output.ass",
                "--keep-watermarks",
            ])
            .output()?,
        0,
    );
    let output = fs::read_to_string(root.join("output.ass"))?;
    assert!(output.contains(r"{\fnمي روم{X}"));
    assert!(output.contains("می‌روم"));
    Ok(())
}

#[test]
fn status_reports_invalid_format_before_editorial_review_is_complete() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    fs::write(root.join("input.srt"), SOURCE)?;
    status(&prepare(root, Path::new("work"), "utf-8")?, 0);
    let path = root.join("work/project.json");
    let mut project = read(&path)?;
    project["output_format"] = json!("vtt");
    write(&path, &project)?;
    let output = command(root).args(["status", "--work", "work"]).output()?;
    status(&output, 2);
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8(output.stderr)?.contains("output_format"));
    Ok(())
}

#[cfg(unix)]
#[test]
fn prepare_rejects_non_utf8_destination_without_publishing_or_blocking_retry() -> Result<()> {
    use std::os::unix::ffi::OsStringExt;
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    fs::write(root.join("input.srt"), SOURCE)?;
    let work = root.join(std::ffi::OsString::from_vec(b"work-\xff".to_vec()));
    for _ in 0..2 {
        let output = prepare(root, &work, "utf-8")?;
        status(&output, 2);
        assert!(String::from_utf8(output.stderr)?.contains("UTF-8"));
        assert!(!work.exists());
    }
    Ok(())
}
