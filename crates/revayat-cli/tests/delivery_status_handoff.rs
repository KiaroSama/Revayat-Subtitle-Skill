use anyhow::{Result, anyhow};
use revayat_core::{
    render::{
        renderer::{self, RenderEvidence},
        sampling,
    },
    runtime::{digest_bytes, files},
    subtitle_formats::document,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

const SOURCE: &str = "1\n00:00:01,000 --> 00:00:02,000\nFirst source line\n\n2\n00:00:04,000 --> 00:00:05,000\nSecond source line\n";
const PNG: &str = "89504e470d0a1a0a0000000d4948445200000001000000010802000000907753de0000000c49444154789c63606060000000040001f61738550000000049454e44ae426082";

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_revayat-subtitle"));
    command
        .current_dir(root)
        .env("REVAYAT_LOG_DIR", root.join("logs"))
        .env_remove("REVAYAT_LOG_LEVEL")
        .stdin(Stdio::null());
    command
}

fn assert_status(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn report(output: Output, status: i32) -> Result<Value> {
    assert_status(&output, status);
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn read(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn write(path: &Path, data: &Value) -> Result<()> {
    files::write_json(path, data).map_err(|e| anyhow!(e))
}

#[derive(Debug, PartialEq, Eq)]
enum SnapshotEntry {
    Directory,
    File(Vec<u8>),
    Link(PathBuf),
}

fn snapshot(root: &Path) -> Result<BTreeMap<PathBuf, SnapshotEntry>> {
    let mut result = BTreeMap::new();
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry?;
        let value = if entry.file_type().is_symlink() {
            SnapshotEntry::Link(fs::read_link(entry.path())?)
        } else if entry.file_type().is_dir() {
            SnapshotEntry::Directory
        } else {
            SnapshotEntry::File(fs::read(entry.path())?)
        };
        result.insert(entry.path().strip_prefix(root)?.into(), value);
    }
    Ok(result)
}

struct Work(tempfile::TempDir);

impl Work {
    fn prepare() -> Result<Self> {
        let w = Self(tempfile::tempdir()?);
        fs::write(w.root().join("S01E01.srt"), SOURCE)?;
        let output = w.run(&[
            "prepare",
            "S01E01.srt",
            "--work",
            "work",
            "--series",
            "Test Series",
            "--season",
            "1",
        ])?;
        assert_status(&output, 0);
        Ok(w)
    }

    fn root(&self) -> &Path {
        self.0.path()
    }
    fn work(&self) -> PathBuf {
        self.root().join("work")
    }
    fn run(&self, argv: &[&str]) -> Result<Output> {
        Ok(command(self.root()).args(argv).output()?)
    }

    fn complete(&self) -> Result<()> {
        let project = self.work().join("project.json");
        let mut data = read(&project)?;
        data["episodes"] = json!([{"id":"S01E01","base":"s0001","alternates":[],"comparison":"Only available source."}]);
        write(&project, &data)?;
        write(
            &self.work().join("glossary.json"),
            &json!({"series":"Test Series","terms_reviewed":true,
            "research":[{"url":"https://example.org/fixture","note":"Synthetic test reference"}],"terms":[]}),
        )?;
        let worksheet = self.work().join("worksheets/s0001.json");
        let mut sheet = read(&worksheet)?;
        for (row, translation) in sheet
            .as_array_mut()
            .expect("worksheet rows")
            .iter_mut()
            .zip(["خط اول", "خط دوم"])
        {
            row["reviewed"] = json!(true);
            row["text"] = json!(translation);
        }
        write(&worksheet, &sheet)
    }

    fn build(&self) -> Result<Value> {
        report(self.run(&["build", "--work", "work"])?, 0)
    }
    fn status(&self) -> Result<Output> {
        self.run(&["status", "--work", "work"])
    }
    fn handoff(&self) -> Result<Output> {
        self.run(&[
            "handoff",
            "--work",
            "work",
            "--assignment",
            "assignment.json",
            "--result",
            "result.json",
        ])
    }

    fn proposals(&self) -> Result<(Value, Value)> {
        self.complete()?;
        let project = read(&self.work().join("project.json"))?;
        let sheet = read(&self.work().join("worksheets/s0001.json"))?;
        let assignment = json!({"version":1,"id":"worker-a","source":"s0001","episode":"S01E01",
            "source_sha256":project["sources"][0]["sha256"],
            "glossary_sha256":digest_bytes(&fs::read(self.work().join("glossary.json"))?),
            "assigned_ids":[sheet[0]["id"]],"context_ids":[sheet[1]["id"]]});
        let result = json!({"version":1,"assignment_id":"worker-a","status":"complete",
            "source_sha256":assignment["source_sha256"],"glossary_sha256":assignment["glossary_sha256"],"rows":[sheet[0]]});
        write(&self.root().join("assignment.json"), &assignment)?;
        write(&self.root().join("result.json"), &result)?;
        Ok((assignment, result))
    }

    fn evidence(&self, approve: bool) -> Result<Value> {
        self.complete()?;
        let manifest = self.build()?;
        let build = build_path(&manifest);
        let episode = &manifest["episodes"][0];
        let doc = document::parse(
            &fs::read_to_string(build.join(episode["file"].as_str().expect("subtitle path")))?,
            "ass",
        )
        .map_err(|e| anyhow!(e))?;
        let folder = "renders/S01E01-synthetic";
        fs::create_dir_all(build.join(folder))?;
        let png = hex::decode(PNG)?;
        let mut frames = sampling::samples(
            &doc,
            true,
            &sampling::changed_indices(episode).map_err(|e| anyhow!(e))?,
        )
        .map_err(|e| anyhow!(e))?;
        for (index, frame) in frames.iter_mut().enumerate() {
            let filename = format!("{folder}/frame-{:04}.png", index + 1);
            fs::write(build.join(&filename), &png)?;
            frame["file"] = json!(filename);
            frame["width"] = json!(1);
            frame["height"] = json!(1);
            frame["sha256"] = json!(digest_bytes(&png));
        }
        let recipe = json!({"sampler":sampling::SAMPLER_VERSION,"profile":"rgb24-v1","fonts":[],
            "requested_fonts":sampling::requested_fonts(&doc).map_err(|e| anyhow!(e))?,"video":null,
            "renderer":{"name":"authored-fixture","version":"synthetic; not executed","bytes":0,"sha256":digest_bytes(b"")}});
        renderer::write_evidence(
            build,
            &RenderEvidence {
                manifest: &manifest,
                episode,
                document: &doc,
                frames: &frames,
                all_cues: true,
                recipe: &recipe,
            },
        )
        .map_err(|e| anyhow!(e))?;
        if approve {
            let path = build.join("renders/S01E01.json");
            let mut review = read(&path)?;
            for frame in review["frames"].as_array_mut().expect("render frames") {
                frame["reviewed"] = json!(true);
                frame["note"] = json!("Synthetic mechanical gate fixture");
            }
            write(&path, &review)?;
        }
        Ok(manifest)
    }
}

fn build_path(data: &Value) -> &Path {
    Path::new(data["build"].as_str().expect("build path"))
}

#[test]
fn prepare_records_both_source_cues_and_preserves_input() -> Result<()> {
    let w = Work::prepare()?;
    let sheet = read(&w.work().join("worksheets/s0001.json"))?;
    assert_eq!(sheet.as_array().map(Vec::len), Some(2));
    assert_eq!(sheet[0]["source_text"], "First source line");
    assert_eq!(sheet[1]["reviewed"], false);
    assert_eq!(fs::read_to_string(w.root().join("S01E01.srt"))?, SOURCE);
    Ok(())
}

#[test]
fn build_refuses_unreviewed_worksheet_without_creating_edition() -> Result<()> {
    let w = Work::prepare()?;
    let before = snapshot(&w.work())?;
    assert_status(&w.run(&["build", "--work", "work"])?, 2);
    assert_eq!(snapshot(&w.work())?, before);
    Ok(())
}

#[test]
fn build_is_repeatable_and_does_not_rewrite_source_or_review() -> Result<()> {
    let w = Work::prepare()?;
    w.complete()?;
    let before = snapshot(&w.work())?;
    let first = w.build()?;
    let again = w.build()?;
    assert_eq!(first["identity"], again["identity"]);
    assert_eq!(first["reviewed_cues"], 2);
    let after = snapshot(&w.work())?;
    for (path, bytes) in before {
        assert_eq!(after.get(&path), Some(&bytes));
    }
    Ok(())
}

#[test]
fn convert_writes_ass_with_bold_italics_and_default_font() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    fs::write(
        scratch.path().join("source.srt"),
        SOURCE.replace("First source line", "<b>پررنگ</b> <i>مورب</i>"),
    )?;
    let output = command(scratch.path())
        .args(["convert", "source.srt", "--out", "out.ass"])
        .output()?;
    assert_eq!(report(output, 0)?["cues"], 2);
    let ass = fs::read_to_string(scratch.path().join("out.ass"))?;
    for mark in ["B Koodak", r"{\b1}", r"{\i1}", "پررنگ", "مورب"] {
        assert!(ass.contains(mark), "missing {mark}");
    }
    Ok(())
}

#[test]
fn translator_ass_keeps_source_for_review_and_accepts_paths_with_spaces() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    fs::write(scratch.path().join("source file.srt"), SOURCE)?;
    let output = command(scratch.path())
        .args([
            "translator-ass",
            "source file.srt",
            "--out",
            "draft file.ass",
        ])
        .output()?;
    assert_eq!(report(output, 0)?["cues"], 2);
    let text = fs::read_to_string(scratch.path().join("draft file.ass"))?;
    assert!(text.contains("First source line"));
    assert!(text.contains("Second source line"));
    Ok(())
}

#[test]
fn status_pending_leaves_import_unchanged() -> Result<()> {
    let w = Work::prepare()?;
    let before = snapshot(&w.work())?;
    let pending = report(w.status()?, 1)?;
    assert_eq!(pending["pending_cues"], 2);
    assert_eq!(pending["state"], "pending");
    assert_eq!(snapshot(&w.work())?, before);
    Ok(())
}

#[test]
fn status_current_build_without_render_is_pending() -> Result<()> {
    let w = Work::prepare()?;
    w.complete()?;
    let build = w.build()?;
    let status = report(w.status()?, 1)?;
    assert_eq!(status["editions"][0]["identity"], build["identity"]);
    assert_eq!(status["editions"][0]["visual_pending"], 1);
    assert_eq!(status["pending_cues"], 0);
    Ok(())
}

#[test]
fn status_with_render_but_without_approval_is_pending() -> Result<()> {
    let w = Work::prepare()?;
    w.evidence(false)?;
    let status = report(w.status()?, 1)?;
    assert!(
        status["editions"][0]["visual_pending"]
            .as_u64()
            .is_some_and(|n| n > 0)
    );
    Ok(())
}

#[test]
fn status_complete_is_read_only_and_does_not_certify_translation() -> Result<()> {
    let w = Work::prepare()?;
    w.evidence(true)?;
    let before = snapshot(&w.work())?;
    let status = report(w.status()?, 0)?;
    assert_eq!(status["state"], "complete");
    assert_eq!(status["linguistic_quality_certified"], false);
    assert_eq!(snapshot(&w.work())?, before);
    Ok(())
}

#[test]
fn status_detects_changed_source_hash_without_rewriting_workspace() -> Result<()> {
    let w = Work::prepare()?;
    let project = read(&w.work().join("project.json"))?;
    let relative = project["sources"][0]["file"].as_str().expect("source file");
    fs::write(w.work().join(relative), SOURCE.replace("First", "Changed"))?;
    let before = snapshot(&w.work())?;
    assert_status(&w.status()?, 2);
    assert_eq!(snapshot(&w.work())?, before);
    Ok(())
}

#[test]
fn changed_review_mapping_invalidates_status() -> Result<()> {
    let w = Work::prepare()?;
    let path = w.work().join("worksheets/s0001.json");
    let mut sheet = read(&path)?;
    sheet.as_array_mut().expect("rows").swap(0, 1);
    write(&path, &sheet)?;
    let before = snapshot(&w.work())?;
    assert_status(&w.status()?, 2);
    assert_eq!(snapshot(&w.work())?, before);
    Ok(())
}

#[test]
fn glossary_change_marks_previously_reviewed_edition_stale() -> Result<()> {
    let w = Work::prepare()?;
    w.evidence(true)?;
    let path = w.work().join("glossary.json");
    let mut glossary = read(&path)?;
    glossary["research"][0]["note"] = json!("Updated source note");
    write(&path, &glossary)?;
    let status = report(w.status()?, 1)?;
    assert_eq!(status["editions"][0]["state"], "stale");
    Ok(())
}

#[test]
fn qa_reports_reviewed_frames_without_writing_changes() -> Result<()> {
    let w = Work::prepare()?;
    let build = w.evidence(true)?;
    let before = snapshot(&w.work())?;
    let output = command(w.root())
        .args(["qa", "--build"])
        .arg(build_path(&build))
        .output()?;
    let qa = report(output, 0)?;
    assert_eq!(qa["ok"], true);
    assert_eq!(qa["reviewed_cues"], 2);
    assert!(qa["reviewed_frames"].as_u64().is_some_and(|n| n >= 2));
    assert_eq!(snapshot(&w.work())?, before);
    Ok(())
}

fn package(w: &Work, build: &Value, out: &Path) -> Result<Output> {
    Ok(command(w.root())
        .args(["package", "--build"])
        .arg(build_path(build))
        .arg("--out")
        .arg(out)
        .output()?)
}

#[test]
fn package_produces_repeatable_zip_inside_sub_and_refuses_clobber() -> Result<()> {
    let w = Work::prepare()?;
    let build = w.evidence(true)?;
    let first = w.root().join("requested leaf.zip");
    let second = w.root().join("delivery again.zip");
    report(package(&w, &build, &first)?, 0)?;
    let bytes = fs::read(&first)?;
    assert!(bytes.starts_with(b"PK\x03\x04"));
    assert!(
        bytes
            .windows(b"Sub/S01E01.ass".len())
            .any(|w| w == b"Sub/S01E01.ass")
    );
    report(package(&w, &build, &second)?, 0)?;
    assert_eq!(fs::read(second)?, bytes);
    assert_status(&package(&w, &build, &first)?, 2);
    assert_eq!(fs::read(first)?, bytes);
    Ok(())
}

#[test]
fn zip_preserves_already_existing_competing_file() -> Result<()> {
    let w = Work::prepare()?;
    let build = w.evidence(true)?;
    let out = w.root().join("competing.zip");
    fs::write(&out, b"foreign archive")?;
    let result = package(&w, &build, &out)?;
    assert_status(&result, 2);
    assert!(String::from_utf8(result.stderr)?.contains("already exists"));
    assert_eq!(fs::read(out)?, b"foreign archive");
    Ok(())
}

#[test]
fn package_rejects_non_zip_and_allows_retry_under_a_new_name() -> Result<()> {
    let w = Work::prepare()?;
    let build = w.evidence(true)?;
    let out = w.root().join("wrong.txt");
    assert_status(&package(&w, &build, &out)?, 2);
    assert!(!out.exists());
    report(package(&w, &build, &w.root().join("right.zip"))?, 0)?;
    Ok(())
}

#[test]
fn unapproved_render_does_not_publish_partial_archive() -> Result<()> {
    let w = Work::prepare()?;
    let build = w.evidence(false)?;
    let out = w.root().join("unapproved.zip");
    assert_status(&package(&w, &build, &out)?, 2);
    assert!(!out.exists());
    assert!(
        !fs::read_dir(w.root())?
            .filter_map(std::result::Result::ok)
            .any(|e| e.file_name().to_string_lossy().starts_with(".package-"))
    );
    Ok(())
}

#[test]
fn f13_dangling_output_link_never_redirects_package() -> Result<()> {
    let w = Work::prepare()?;
    let build = w.evidence(true)?;
    let link = w.root().join("requested.zip");
    let missing = w.root().join("not-created.zip");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&missing, &link)?;
    #[cfg(windows)]
    {
        fs::create_dir(&missing)?;
        let created = Command::new(std::env::var_os("COMSPEC").unwrap_or_else(|| "cmd.exe".into()))
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&link)
            .arg(&missing)
            .stdin(Stdio::null())
            .output()?;
        assert!(
            created.status.success(),
            "{}",
            String::from_utf8_lossy(&created.stderr)
        );
        fs::remove_dir(&missing)?;
    }
    assert!(fs::symlink_metadata(&link).is_ok());
    assert!(!link.exists());
    let output = package(&w, &build, &link)?;
    assert_status(&output, 2);
    assert!(String::from_utf8(output.stderr)?.contains("already exists"));
    assert_eq!(fs::read_link(link)?, missing);
    assert!(!missing.exists());
    Ok(())
}

#[test]
fn changed_render_bytes_make_status_invalid_and_keep_archive_absent() -> Result<()> {
    let w = Work::prepare()?;
    let build = w.evidence(true)?;
    fs::write(
        build_path(&build).join("renders/S01E01-synthetic/frame-0001.png"),
        b"not a png",
    )?;
    assert_status(&w.status()?, 2);
    let out = w.root().join("broken.zip");
    assert_status(&package(&w, &build, &out)?, 2);
    assert!(!out.exists());
    Ok(())
}

#[test]
fn integer_review_flag_cannot_approve_frame() -> Result<()> {
    let w = Work::prepare()?;
    let build = w.evidence(true)?;
    let path = build_path(&build).join("renders/S01E01.json");
    let mut data = read(&path)?;
    data["frames"][0]["reviewed"] = json!(1);
    write(&path, &data)?;
    assert_status(&package(&w, &build, &w.root().join("wrong.zip"))?, 2);
    Ok(())
}

#[test]
fn separate_identical_frames_pass_but_hardlinked_frames_fail() -> Result<()> {
    let w = Work::prepare()?;
    let build = w.evidence(true)?;
    let out = w.root().join("separate.zip");
    report(package(&w, &build, &out)?, 0)?;
    let dir = build_path(&build).join("renders/S01E01-synthetic");
    let second = dir.join("frame-0002.png");
    fs::remove_file(&second)?;
    fs::hard_link(dir.join("frame-0001.png"), &second)?;
    let refused = package(&w, &build, &w.root().join("aliased.zip"))?;
    assert_status(&refused, 2);
    assert!(String::from_utf8(refused.stderr)?.contains("aliased"));
    Ok(())
}

#[test]
fn complete_handoff_is_not_imported_or_certified() -> Result<()> {
    let w = Work::prepare()?;
    w.proposals()?;
    let before = snapshot(&w.work())?;
    let result = report(w.handoff()?, 0)?;
    assert_eq!(result["coordinator_review_required"], true);
    assert_eq!(result["linguistic_quality_certified"], false);
    assert_eq!(snapshot(&w.work())?, before);
    Ok(())
}

#[test]
fn pending_handoff_returns_one_and_does_not_import() -> Result<()> {
    let w = Work::prepare()?;
    let (_, mut result) = w.proposals()?;
    result["status"] = json!("pending");
    result["rows"][0]["reviewed"] = json!(false);
    result["rows"][0]["text"] = Value::Null;
    write(&w.root().join("result.json"), &result)?;
    let before = snapshot(&w.work())?;
    assert_eq!(report(w.handoff()?, 1)?["pending_rows"], 1);
    assert_eq!(snapshot(&w.work())?, before);
    Ok(())
}

fn bad_result(mutate: impl FnOnce(&mut Value)) -> Result<()> {
    let w = Work::prepare()?;
    let (_, mut proposal) = w.proposals()?;
    mutate(&mut proposal);
    write(&w.root().join("result.json"), &proposal)?;
    let before = snapshot(&w.work())?;
    let output = w.handoff()?;
    assert_status(&output, 2);
    let error = String::from_utf8(output.stderr)?;
    assert!(!error.contains("First source line"));
    assert!(!error.contains("changed private body"));
    assert_eq!(snapshot(&w.work())?, before);
    Ok(())
}

#[test]
fn handoff_rejects_stale_glossary_digest() -> Result<()> {
    bad_result(|v| v["glossary_sha256"] = json!("0".repeat(64)))
}
#[test]
fn handoff_rejects_stale_source_digest() -> Result<()> {
    bad_result(|v| v["source_sha256"] = json!("0".repeat(64)))
}
#[test]
fn handoff_rejects_missing_assigned_row() -> Result<()> {
    bad_result(|v| v["rows"] = json!([]))
}
#[test]
fn handoff_rejects_edit_to_context_only_row() -> Result<()> {
    bad_result(|v| v["rows"][0]["id"] = json!("s0001:c000002"))
}
#[test]
fn handoff_rejects_unknown_fields() -> Result<()> {
    bad_result(|v| v["extra"] = json!(true))
}
#[test]
fn handoff_rejects_boolean_status() -> Result<()> {
    bad_result(|v| v["status"] = json!(true))
}
#[test]
fn handoff_rejects_modified_source_body() -> Result<()> {
    bad_result(|v| v["rows"][0]["source_text"] = json!("changed private body"))
}
#[test]
fn handoff_rejects_duplicate_result_row() -> Result<()> {
    bad_result(|v| v["rows"] = json!([v["rows"][0], v["rows"][0]]))
}

#[test]
fn handoff_rejects_a_self_referential_alternate() -> Result<()> {
    bad_result(|v| {
        let row = &mut v["rows"][0];
        row["action"] = json!("alternate");
        row["note"] = json!("Duplicate source wording");
        row["text"] = Value::Null;
        row["links"] = json!([row["id"]]);
    })
}

#[test]
fn handoff_rejects_exclusion_without_an_editorial_note() -> Result<()> {
    bad_result(|v| {
        v["rows"][0]["action"] = json!("credit");
        v["rows"][0]["text"] = Value::Null;
        v["rows"][0]["note"] = json!("");
    })
}

#[test]
fn handoff_rejects_timing_change_without_a_note() -> Result<()> {
    bad_result(|v| v["rows"][0]["start_ms"] = json!(1100))
}

#[test]
fn handoff_rejects_overlapping_assignments_before_results() -> Result<()> {
    let w = Work::prepare()?;
    let (mut assignment, _) = w.proposals()?;
    assignment["id"] = json!("worker-b");
    write(&w.root().join("second.json"), &assignment)?;
    let output = w.run(&[
        "handoff",
        "--work",
        "work",
        "--assignment",
        "assignment.json",
        "--assignment",
        "second.json",
        "--result",
        "result.json",
    ])?;
    assert_status(&output, 2);
    assert!(String::from_utf8(output.stderr)?.contains("overlap"));
    Ok(())
}

#[test]
fn doctor_returns_not_ready_for_missing_ffmpeg() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let output = command(scratch.path())
        .args(["doctor", "--ffmpeg", "missing-ffmpeg.exe"])
        .output()?;
    assert_eq!(report(output, 1)?["ready"], false);
    Ok(())
}

#[test]
fn lint_fix_normalizes_arabic_letter_forms_in_place() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let input = scratch.path().join("letters.txt");
    fs::write(&input, "ي ك")?;
    assert_status(
        &command(scratch.path())
            .args(["lint", "--fix", "--in-place", "letters.txt"])
            .output()?,
        0,
    );
    assert_eq!(fs::read_to_string(input)?, "ی ک");
    Ok(())
}

#[test]
fn lookup_json_uses_explicit_glossary_for_canon_term() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    fs::write(scratch.path().join("glossary.tsv"), "word\tمعنی\n")?;
    fs::write(scratch.path().join("idioms.tsv"), "term\tمعنا\n")?;
    let output = command(scratch.path())
        .args(["lookup", "--json", "--data-dir", ".", "word"])
        .output()?;
    assert!(report(output, 0)?.to_string().contains("معنی"));
    Ok(())
}

#[test]
fn score_reports_missing_answers_without_certification() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    fs::write(scratch.path().join("answers.json"), "{}")?;
    let output = command(scratch.path())
        .args(["score", "--answers", "answers.json"])
        .output()?;
    let value = report(output, 1)?;
    assert_eq!(value["linguistic_quality_certified"], false);
    assert!(
        value["counts"]["missing"]
            .as_u64()
            .is_some_and(|count| count > 0)
    );
    Ok(())
}

#[test]
fn install_help_exposes_explicit_recovery_and_uninstall() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let output = command(scratch.path())
        .args(["install", "--help"])
        .output()?;
    assert_status(&output, 0);
    let text = String::from_utf8(output.stdout)?;
    for flag in ["--agent", "--recovery-dir", "--recover", "--uninstall"] {
        assert!(text.contains(flag));
    }
    Ok(())
}

#[test]
fn sync_help_exposes_audio_source_and_report_flags() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let output = command(scratch.path()).args(["sync", "--help"]).output()?;
    assert_status(&output, 0);
    let text = String::from_utf8(output.stdout)?;
    for flag in ["--wav", "--video", "--json-report"] {
        assert!(text.contains(flag), "no {flag} in {text}");
    }
    Ok(())
}

#[test]
fn version_prints_the_v3_release_without_a_log() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let output = command(scratch.path()).arg("--version").output()?;
    assert_status(&output, 0);
    assert_eq!(
        String::from_utf8(output.stdout)?.trim(),
        "revayat-subtitle 3.0.5"
    );
    assert!(!scratch.path().join("logs").exists());
    Ok(())
}

#[test]
fn supervise_can_forward_version_output_from_a_real_cli_child() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let output = command(scratch.path())
        .args([
            "supervise",
            "--timeout",
            "10",
            "--",
            env!("CARGO_BIN_EXE_revayat-subtitle"),
            "--version",
        ])
        .output()?;
    assert_status(&output, 0);
    assert_eq!(
        String::from_utf8(output.stdout)?.trim(),
        "revayat-subtitle 3.0.5"
    );
    Ok(())
}
