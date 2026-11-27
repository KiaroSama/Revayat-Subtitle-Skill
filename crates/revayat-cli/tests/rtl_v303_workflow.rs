//! Public CLI coverage for scoped directional-control repair.

use anyhow::{Result, anyhow};
use revayat_core::{markup, runtime::files, subtitle_formats::document};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
};

fn command(root: &Path, arguments: &[&str]) -> Result<Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_revayat-subtitle"))
        .current_dir(root)
        .env("REVAYAT_LOG_DIR", root.join("logs"))
        .env_remove("REVAYAT_LOG_LEVEL")
        .stdin(Stdio::null())
        .args(arguments)
        .output()?)
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    files::write_json(path, value).map_err(|error| anyhow!(error))
}

#[test]
fn reviewed_build_accepts_valid_sibling_embeddings() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    let text = "\u{202b}سلام\u{202c} \u{202b}دنیا\u{202c}";
    let source = format!("1\n00:00:00,000 --> 00:00:02,000\n{text}\n");
    fs::write(root.join("S01E01.srt"), &source)?;
    success(&command(
        root,
        &[
            "prepare",
            "S01E01.srt",
            "--work",
            "work",
            "--series",
            "Authored RTL Audit",
            "--season",
            "1",
            "--output-format",
            "srt",
        ],
    )?);
    let work = root.join("work");
    let project_path = work.join("project.json");
    let mut project: Value = serde_json::from_slice(&fs::read(&project_path)?)?;
    project["episodes"] = json!([{
        "id": "S01E01", "base": "s0001", "alternates": [],
        "comparison": "Single authored directional-control fixture"
    }]);
    write_json(&project_path, &project)?;
    write_json(
        &work.join("glossary.json"),
        &json!({
            "series": "Authored RTL Audit", "terms_reviewed": true, "terms": [],
            "research": [{"url": "https://www.unicode.org/reports/tr9/", "note": "Authored balanced-scope regression"}]
        }),
    )?;
    let sheet_path = work.join("worksheets/s0001.json");
    let mut sheet: Value = serde_json::from_slice(&fs::read(&sheet_path)?)?;
    for row in sheet.as_array_mut().expect("worksheet rows") {
        row["reviewed"] = json!(true);
        row["action"] = json!("preserve");
    }
    write_json(&sheet_path, &sheet)?;
    let output = command(root, &["build", "--work", "work"])?;
    success(&output);
    let manifest: Value = serde_json::from_slice(&output.stdout)?;
    let build = Path::new(manifest["build"].as_str().expect("build path"));
    let emitted = build.join(
        manifest["episodes"][0]["file"]
            .as_str()
            .expect("episode file"),
    );
    let document =
        document::parse(&fs::read_to_string(emitted)?, "srt").map_err(anyhow::Error::msg)?;
    assert_eq!(document.cues.len(), 1);
    assert!(document.cues[0].text.contains(text));
    markup::validate_bidi(document.cues[0].text.clone(), "srt".into())
        .map_err(anyhow::Error::msg)?;
    assert_eq!(fs::read_to_string(root.join("S01E01.srt"))?, source);
    Ok(())
}

#[test]
fn convert_preserves_a_balanced_inner_isolate_while_removing_a_stray_opener() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    let source = "1\n00:00:00,000 --> 00:00:02,000\nسلام \u{2066}A 12\u{2069} B\u{202b}\n";
    fs::write(root.join("input.srt"), source)?;
    success(&command(
        root,
        &[
            "convert",
            "input.srt",
            "--out",
            "output.ass",
            "--max-chars",
            "4096",
        ],
    )?);
    let output = document::parse(&fs::read_to_string(root.join("output.ass"))?, "ass")
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        output.cues[0].text,
        "\u{200f}\u{2067}سلام \u{2066}A 12\u{2069} B\u{2069}"
    );
    success(&command(
        root,
        &[
            "convert",
            "output.ass",
            "--out",
            "second.ass",
            "--max-chars",
            "4096",
        ],
    )?);
    let second = document::parse(&fs::read_to_string(root.join("second.ass"))?, "ass")
        .map_err(anyhow::Error::msg)?;
    assert_eq!(second.cues[0].text, output.cues[0].text);
    assert_eq!(fs::read_to_string(root.join("input.srt"))?, source);
    Ok(())
}

#[test]
fn convert_repairs_cross_line_controls_as_one_cue() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    fs::write(
        root.join("input.srt"),
        "1\n00:00:00,000 --> 00:00:02,000\n\u{2066}A 12\nسلام B\u{2069}\u{2069}\n",
    )?;
    success(&command(
        root,
        &[
            "convert",
            "input.srt",
            "--out",
            "output.ass",
            "--max-chars",
            "4096",
        ],
    )?);
    let output = document::parse(&fs::read_to_string(root.join("output.ass"))?, "ass")
        .map_err(anyhow::Error::msg)?;
    assert_eq!(output.cues[0].text, "\u{2066}A 12\\Nسلام B\u{2069}");
    markup::validate_bidi(output.cues[0].text.clone(), "ass".into()).map_err(anyhow::Error::msg)?;
    Ok(())
}
