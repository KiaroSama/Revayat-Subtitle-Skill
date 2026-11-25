use revayat_core::{
    handoff::review_handoff,
    render::renderer,
    runtime::{diagnostic::redact, digest_bytes, files},
    status,
    subtitle_formats::{
        conversion::{self, Options},
        document::{Cue, Document, parse},
    },
    workflow::{
        import::{ImportOptions, prepare},
        workspace::{build, read_record},
    },
};
use serde_json::{Value, json};
use std::fs;

#[test]
fn quoted_credential_redaction_consumes_escaped_quotes() {
    let input = r#"token="first\"second with spaces", password='third\'fourth'; next=visible"#;
    let result = redact(input);
    for secret in ["first", "second", "third", "fourth"] {
        assert!(!result.contains(secret));
    }
    assert!(result.ends_with("next=visible"));
}

#[test]
fn json_writes_respect_the_reader_depth_limit() -> Result<(), String> {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = root.path().join("record.json");
    let mut record = Value::Null;
    for depth in 0..127 {
        record = if depth % 2 == 0 {
            json!({"nested": record})
        } else {
            Value::Array(vec![record])
        };
    }
    files::write_json(&path, &record)?;
    let saved = fs::read(&path).map_err(|e| e.to_string())?;
    assert_eq!(files::read_json(&path, files::MAX_JSON_BYTES)?, record);
    let too_deep = Value::Array(vec![record]);
    assert!(files::write_json(&path, &too_deep).is_err());
    assert_eq!(fs::read(&path).map_err(|e| e.to_string())?, saved);
    Ok(())
}

fn io(error: std::io::Error) -> String {
    error.to_string()
}

fn reviewed_workspace(root: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let source = root.join("S01E01.srt");
    fs::write(
        &source,
        concat!(
            "1\n00:00:01,000 --> 00:00:02,000\nA subtitle.\n\n",
            "2\n00:00:05,000 --> 00:00:06,000\nAnother subtitle.\n",
        ),
    )
    .map_err(io)?;
    let work = root.join("work");
    prepare(
        &[source],
        &work,
        &ImportOptions {
            series: "Review fixture",
            season: 1,
            encoding: "utf-8",
            glossary: None,
            language: "en",
            output_format: "ass",
        },
    )?;
    let mut project = read_record(&work, "project.json")?;
    project["episodes"] = json!([{"id":"S01E01","base":"s0001","alternates":[],"comparison":"Single authored source"}]);
    files::write_json(&work.join("project.json"), &project)?;
    files::write_json(
        &work.join("glossary.json"),
        &json!({"series":"Review fixture","terms_reviewed":true,"terms":[],"research":[{"url":"https://example.org/fixture","note":"Authored fixture"}]}),
    )?;
    let sheet = "worksheets/s0001.json";
    let mut rows = read_record(&work, sheet)?;
    for row in rows.as_array_mut().ok_or("Expected worksheet rows")? {
        row["action"] = json!("preserve");
        row["reviewed"] = json!(true);
    }
    files::write_json(&work.join(sheet), &rows)?;
    Ok(work)
}

#[test]
fn status_recognizes_an_edition_built_with_an_output_override() -> Result<(), String> {
    let root = tempfile::tempdir().map_err(io)?;
    let work = reviewed_workspace(root.path())?;
    build(&work, Some("srt"))?;
    let report = status::report(&work)?;
    assert_eq!(report["editions"][0]["state"], "pending");
    assert_eq!(report["editions"][0]["episodes"], 1);
    assert_eq!(report["editions"][0]["visual_pending"], 1);
    Ok(())
}

#[test]
fn handoff_checks_existing_links_against_proposed_exclusions() -> Result<(), String> {
    let root = tempfile::tempdir().map_err(io)?;
    let work = reviewed_workspace(root.path())?;
    let project = read_record(&work, "project.json")?;
    let sheet = "worksheets/s0001.json";
    let mut rows = read_record(&work, sheet)?;
    rows[0]["links"] = json!([format!(
        "s0001:{}",
        rows[1]["id"].as_str().ok_or("Expected cue id")?
    )]);
    files::write_json(&work.join(sheet), &rows)?;
    let assignment = root.path().join("assignment.json");
    let result = root.path().join("result.json");
    let source_hash = &project["sources"][0]["sha256"];
    let glossary_hash = digest_bytes(&fs::read(work.join("glossary.json")).map_err(io)?);
    files::write_json(
        &assignment,
        &json!({"version":1,"id":"worker-a","source":"s0001","episode":"S01E01",
        "source_sha256":source_hash,"glossary_sha256":glossary_hash,"assigned_ids":[rows[1]["id"]],"context_ids":[rows[0]["id"]]}),
    )?;
    let mut row = rows[1].clone();
    row["action"] = json!("credit");
    row["note"] = json!("Exclude this line for the review fixture");
    files::write_json(
        &result,
        &json!({"version":1,"assignment_id":"worker-a","status":"complete",
        "source_sha256":source_hash,"glossary_sha256":glossary_hash,"rows":[row]}),
    )?;
    assert!(
        review_handoff(&work, &[assignment], &[result])
            .is_err_and(|e| e.contains("excluded decisions"))
    );
    Ok(())
}

#[test]
fn a_failed_second_sample_leaves_no_new_render_directory() -> Result<(), String> {
    use revayat_core::process_control::{RunOptions, run_owned};
    use std::{path::Path, time::Duration};

    let root = tempfile::tempdir().map_err(io)?;
    let work = reviewed_workspace(root.path())?;
    let built = build(&work, Some("srt"))?;
    let output = Path::new(built["build"].as_str().ok_or("Expected build path")?);
    let initial = renderer::render(output, "S01E01", None, None, None, true)?;
    let renders = output.join("renders");
    let entry_count = fs::read_dir(&renders).map_err(io)?.count();
    let review = fs::read(renders.join("S01E01.json")).map_err(io)?;
    let frame = output.join(
        initial["frames"][0]["file"]
            .as_str()
            .ok_or("Expected frame path")?,
    );
    let pixels = fs::read(&frame).map_err(io)?;
    let ffmpeg = renderer::ffmpeg_path(None)?;
    let command = std::iter::once(ffmpeg.to_str().ok_or("Expected UTF-8 FFmpeg path")?)
        .chain([
            "-nostdin",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=s=64x64:d=3",
            "-c:v",
            "ffv1",
            "short.mkv",
        ])
        .map(str::to_owned)
        .collect::<Vec<_>>();
    run_owned(
        &command,
        &RunOptions {
            cwd: Some(root.path()),
            timeout: Duration::from_secs(15),
            ..RunOptions::default()
        },
    )
    .map_err(|e| e.to_string())?;
    let result = renderer::render(
        output,
        "S01E01",
        None,
        Some(&root.path().join("short.mkv")),
        None,
        true,
    );
    assert!(result.is_err_and(|e| e.contains("produced no image")));
    assert_eq!(fs::read_dir(&renders).map_err(io)?.count(), entry_count);
    assert_eq!(fs::read(renders.join("S01E01.json")).map_err(io)?, review);
    assert_eq!(fs::read(frame).map_err(io)?, pixels);
    Ok(())
}

#[test]
fn long_primary_cue_keeps_tie_priority_across_sequential_donors() -> Result<(), String> {
    let cue = |id: String, start, end| Cue {
        text: format!("Spoken line {id}."),
        id,
        start,
        end,
        ..Cue::default()
    };
    let sequential: Vec<_> = (0..2_000)
        .map(|index| cue(format!("short-{index}"), 1_000 * index, 1_000 * index + 400))
        .collect();
    let primary = Document {
        kind: "srt".into(),
        cues: std::iter::once(cue("long".into(), 0, 2_000_000))
            .chain(sequential.iter().cloned())
            .collect(),
        ..Document::default()
    };
    let donor = Document {
        kind: "srt".into(),
        cues: sequential.into_iter().rev().collect(),
        ..Document::default()
    };
    let (edition, stats) = conversion::build_translator_ass(
        &primary,
        &[("Donor".into(), donor)],
        true,
        &Options {
            direction: "ltr",
            ..Options::default()
        },
    )?;
    assert_eq!(stats["placeholder_slots"], 0);
    let references = &edition.source_comments["long"];
    assert_eq!(references.len(), 2_001);
    assert_eq!(references[1], "Donor: Spoken line short-1999.");
    assert_eq!(
        references.last().map(String::as_str),
        Some("Donor: Spoken line short-0.")
    );
    Ok(())
}

#[test]
fn srt_conversion_validates_braces_before_watermark_removal() -> Result<(), String> {
    let source = |text| format!("1\n00:00:01,000 --> 00:00:02,000\n{text}\n");
    let invalid = parse(&source("{@channel}"), "srt")?;
    assert!(conversion::convert(&invalid, &Options::default()).is_err());
    let valid = parse(&source("@channel"), "srt")?;
    let (edition, removed) = conversion::convert(&valid, &Options::default())?;
    assert!(edition.cues.is_empty());
    assert_eq!(removed[0]["reason"], "telegram-handle");
    Ok(())
}
