use revayat_core::{
    markup::{clean_empty_lines, remap_resets, rtl, uncomment, validate_bidi},
    runtime::{self, files},
    subtitle_formats::{document, effective_times, visible},
    validation,
    workflow::{contract::path_marker, import, review, workspace},
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

type Check = Result<(), String>;
const RLM: char = '\u{200f}';
const RLE: char = '\u{202b}';

fn srt(text: &str) -> String {
    format!("1\n00:00:01,000 --> 00:00:02,000\n{text}\n")
}

fn ass(text: &str) -> Result<document::Document, String> {
    let mut doc = document::ass_template(None, "Fixture", 1920, 1080)?;
    doc.cues.push(document::Cue {
        id: "c000001".into(),
        start: 1000,
        end: 2000,
        text: text.into(),
        ..Default::default()
    });
    doc.cues[0].fields.insert("style".into(), "Default".into());
    Ok(doc)
}

fn edit(source: &str, text: &str) -> Value {
    json!({"id":"c000001","source_text":source,"text":text,"reviewed":true,
        "action":"edit","start_ms":1000,"end_ms":2000,"links":[]})
}

fn err<T: std::fmt::Debug>(result: Result<T, String>, message: &str) {
    let actual = result.expect_err(message);
    assert!(
        actual.contains(message),
        "{actual:?} must contain {message:?}"
    );
}

fn work(text: &str) -> Result<(tempfile::TempDir, PathBuf, PathBuf), String> {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = root.path().join("input.srt");
    let work = root.path().join("review work");
    fs::write(&path, srt(text)).map_err(|e| e.to_string())?;
    import::prepare(
        std::slice::from_ref(&path),
        &work,
        &import::ImportOptions {
            series: "Authored",
            season: 1,
            encoding: "utf-8",
            glossary: None,
            language: "fa",
            output_format: "srt",
        },
    )?;
    let mut project = workspace::read_record(&work, "project.json")?;
    project["episodes"] = json!([{"id":"S01E01","base":"s0001","comparison":"Authored source"}]);
    files::write_json(&work.join("project.json"), &project)?;
    files::write_json(
        &work.join("glossary.json"),
        &json!({"series":"Authored","terms_reviewed":true,
        "terms":[],"research":[{"url":"https://example.org","note":"Authored fixture"}]}),
    )?;
    Ok((root, path, work))
}

#[test]
fn bidi_only_blank_line_keeps_its_balanced_scope() -> Check {
    let source = "Hello\\N\u{2067}\\Nسلام\u{2069}";
    let (kept, _) = review::reviewed_cues(&ass(source)?, &[edit(source, source)], "fa")?;
    let text = &kept[0].text;
    assert_eq!(visible(text.clone(), "ass".into())?, "Hello\nسلام");
    assert_eq!(text.matches('\u{2067}').count(), 1);
    assert_eq!(text.matches('\u{2069}').count(), 1);
    validate_bidi(text.clone(), "ass".into())
}

#[test]
fn donor_reset_remaps_only_active_arguments() -> Check {
    let mut base = ass("Base")?;
    let donor = ass(r"{\rDefault}Use the literal \rDefault")?;
    let retained = review::donor(&mut base, &donor, donor.cues.clone(), "s0002", false)?;
    assert_eq!(
        retained[0].text,
        r"{\rs0002__Default}Use the literal \rDefault"
    );
    Ok(())
}

#[test]
fn transforms_remap_named_but_not_bare_resets() -> Check {
    assert_eq!(
        remap_resets(
            r"{\t(0,100,\rDefault)\r}x".into(),
            json!({"Default":"Mapped"})
        )?,
        r"{\t(0,100,\rMapped)\r}x"
    );
    err(
        remap_resets(r"{\rUnknown}x".into(), json!({"Default":"Mapped"})),
        "undefined",
    );
    Ok(())
}

#[test]
fn interior_direction_marks_survive() -> Check {
    let normalized = rtl(
        format!("سلام {RLM}OVA برگشت."),
        "ass".into(),
        "rtl".into(),
        false,
    )?;
    assert!(normalized.contains(&format!("سلام {RLM}OVA")));
    assert_eq!(
        rtl(normalized.clone(), "ass".into(), "rtl".into(), false)?,
        normalized
    );
    Ok(())
}

macro_rules! bidi_stable {
    ($($name:ident: ($text:expr, $kind:literal, $dir:literal)),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let normalized = rtl($text.into(), $kind.into(), $dir.into(), false)?;
            if $dir == "ltr" { assert!(!normalized.contains(RLE)); }
            assert_eq!(rtl(normalized.clone(), $kind.into(), $dir.into(), false)?, normalized);
            Ok(())
        }
    )+};
}

bidi_stable! {
    boundary_rtl_marks: (" \u{200f}سلام\u{200f}", "ass", "rtl"),
    duplicated_boundary_marks: ("\u{200f}\u{200f}Hello\u{200f}\u{200f}", "ass", "rtl"),
    ltr_target_persian_word: ("علی", "srt", "ltr"),
    ltr_target_mixed_sentence: ("Meet علی in London.", "srt", "ltr"),
}

#[test]
fn unclosed_isolate_is_refused() -> Check {
    err(
        rtl("\u{2067}سلام".into(), "srt".into(), "rtl".into(), false),
        "Unclosed",
    );
    validate_bidi("\u{2067}سلام\u{2069}".into(), "srt".into())
}

#[test]
fn blank_prose_lines_are_removed_without_losing_resets() -> Check {
    let text = r"\Nسلام\N{\rDefault}\Nدنیا\N";
    let doc = ass(text)?;
    let mut row = edit(text, text);
    let (kept, _) = review::reviewed_cues(&doc, &[row.clone()], "fa")?;
    assert_eq!(visible(kept[0].text.clone(), "ass".into())?, "سلام\nدنیا");
    assert!(kept[0].text.contains(r"{\rDefault}"));
    row["preserve_empty_lines"] = json!(true);
    row["structure_note"] = json!("Intentional spacing");
    let (kept, _) = review::reviewed_cues(&doc, &[row], "fa")?;
    assert!(visible(kept[0].text.clone(), "ass".into())?.contains("\n\n"));
    Ok(())
}

macro_rules! identity_cases {
    ($call:ident; $($name:ident: $text:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check { assert_eq!($call($text)?, $text); Ok(()) }
    )+};
}

fn clean_ass(text: &str) -> Result<String, String> {
    clean_empty_lines(text.into(), "ass".into(), false)
}

identity_cases!(clean_ass;
    vector_keeps_blank_lines: r"{\p1}m 0 0\N\N l 1 1{\p0}سلام",
    karaoke_keeps_blank_lines: r"{\k10}سلام\N\Nدنیا",
    soft_breaks_keep_blank_lines: r"سلام\n\nدنیا",
);

#[test]
fn english_review_requires_a_note_for_direction_override() -> Check {
    let doc = document::parse(&srt("Hello"), "srt")?;
    let mut row = edit("Hello", "Meet علی in London.");
    let (kept, _) = review::reviewed_cues(&doc, &[row.clone()], "en")?;
    assert!(!kept[0].text.contains(RLE));
    assert_eq!(
        visible(kept[0].text.clone(), "srt".into())?,
        "Meet علی in London."
    );
    row["direction"] = json!("rtl");
    err(review::reviewed_cues(&doc, &[row], "en"), "direction_note");
    Ok(())
}

fn srt_visible(text: &str) -> Result<String, String> {
    visible(text.into(), "srt".into())
}

identity_cases!(srt_visible;
    ascii_angle_number: "< 5 >",
    persian_angle_number: "< ۵ >",
    comparison_expression: "x < 5 > y",
);

#[test]
fn supported_tags_and_comment_do_not_erase_literals() -> Check {
    assert_eq!(
        srt_visible("<i>Hi</i> <!--hidden-->x < 5 > y &amp;")?,
        "Hi x < 5 > y &amp;"
    );
    let doc = document::parse(&srt("x < 5 > y"), "srt")?;
    assert_eq!(document::srt_to_ass(doc.cues[0].clone())?.text, "x < 5 > y");
    Ok(())
}

#[test]
fn centiseconds_floor_across_minute_boundary() -> Check {
    assert_eq!(
        effective_times(59999, 60010, "ass".into())?,
        json!({"$tuple":[59990, 60010]})
    );
    Ok(())
}

#[test]
fn srt_to_ass_refuses_collapsed_centisecond_duration() -> Check {
    let doc = document::parse("1\n00:00:01,001 --> 00:00:01,009\nHello\n", "srt")?;
    err(document::srt_to_ass(doc.cues[0].clone()), "collapses");
    Ok(())
}

#[test]
fn srt_to_ass_refuses_unsupported_font_style() -> Check {
    let doc = document::parse(&srt("<font color=\"red\">Hi</font>"), "srt")?;
    err(document::srt_to_ass(doc.cues[0].clone()), "markup");
    Ok(())
}

#[test]
fn repro_tab_after_font_requires_an_attribute() -> Check {
    let unsupported = document::parse(&srt("A<font\tface=\"Arial\">B</font>"), "srt")?;
    err(document::srt_to_ass(unsupported.cues[0].clone()), "markup");

    let literal = "A<font\t>B";
    let doc = document::parse(&srt(literal), "srt")?;
    assert_eq!(document::srt_to_ass(doc.cues[0].clone())?.text, literal);
    Ok(())
}

#[test]
fn repro_replaced_stage_directory_survives_cleanup() -> Result<(), String> {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let stage = revayat_core::workflow::workspace::stage(root.path(), ".owned-")?;
    let path = stage.path().to_owned();
    let moved = root.path().join("moved-stage");
    std::fs::rename(&path, &moved).map_err(|e| e.to_string())?;
    std::fs::create_dir(&path).map_err(|e| e.to_string())?;
    std::fs::write(path.join("unrelated.txt"), "Preserve me").map_err(|e| e.to_string())?;
    drop(stage);
    assert!(moved.is_dir());
    assert_eq!(
        std::fs::read_to_string(path.join("unrelated.txt")).map_err(|e| e.to_string())?,
        "Preserve me"
    );
    Ok(())
}

macro_rules! separators {
    ($($name:ident: $text:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let (_root, source, work) = work("Original")?;
            let sheet = "worksheets/s0001.json";
            let mut rows = workspace::read_record(&work, sheet)?;
            rows[0] = edit("Original", $text);
            rows[0]["preserve_empty_lines"] = json!(true);
            rows[0]["structure_note"] = json!("Intentional layout");
            files::write_json(&work.join(sheet), &rows)?;
            let error = workspace::build(&work, None).expect_err("physical separator must be refused");
            assert!(error.contains("c000001") && error.contains("SRT") && error.contains("separator"), "{error}");
            assert!(!work.join("builds").exists());
            rows[0]["text"] = json!("A\nB");
            files::write_json(&work.join(sheet), &rows)?;
            assert_eq!(workspace::build(&work, None)?["episodes"][0]["cues"], 1);
            assert_eq!(fs::read(source).map_err(|e| e.to_string())?, srt("Original").as_bytes());
            Ok(())
        }
    )+};
}

separators! {
    lf_separator: "A\n\nB",
    crlf_separator: "A\r\n\r\nB",
    cr_separator: "A\r\rB",
    whitespace_separator: "A\n \t\nB",
}

macro_rules! active_comments {
    ($($name:ident: $text:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() {
            let error = uncomment($text.into(), "srt".into()).expect_err("comment must not activate syntax");
            assert!(error.contains("comment") && error.contains("activat"), "{error}");
        }
    )+};
}

active_comments! {
    comment_cannot_create_open_bold: "A<<!--note-->b>B",
    comment_cannot_create_close_bold: "A</<!--note-->b>B",
    comment_cannot_create_break: "A<<!--note-->br>B",
    comment_cannot_create_small: "A<<!--note-->small>B",
    comment_cannot_create_unknown_tag: "A<<!--note-->foo>B",
    comment_cannot_create_numeric_tag: "A<<!--note-->123>B",
    comment_cannot_create_underscore_tag: "A<<!--note-->_foo>B",
    comment_cannot_create_ass_alignment_in_srt: r"A{\a<!--note-->n8}B",
}

macro_rules! remove_comments {
    ($($name:ident: ($kind:literal, $text:literal, $expected:literal)),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            assert_eq!(uncomment($text.into(), $kind.into())?, $expected);
            assert_eq!(uncomment($expected.into(), $kind.into())?, $expected);
            Ok(())
        }
    )+};
}

remove_comments! {
    plain_completed_comment: ("srt", "A<!--note-->B", "AB"),
    bold_around_comment: ("srt", "<b>A<!--note-->B</b>", "<b>AB</b>"),
    unfinished_comment: ("srt", "A<!--unfinished", "A<!--unfinished"),
    non_tag_bang: ("srt", "A<sm<!--note-->all!>B", "A<small!>B"),
    non_tag_space: ("srt", "A< <!--note-->foo>B", "A< foo>B"),
    non_tag_nested_angle: ("srt", "A<<<!--note-->foo>B", "A<<foo>B"),
    literal_ass_escape: ("ass", r"A\{literal}B", r"A\{literal}B"),
    drawing_object_boundary: ("ass", r"{\p1}m 0 0{note}m 1 1{\p0}X", r"{\p1}m 0 0{}m 1 1{\p0}X"),
}

#[test]
fn unsafe_comment_build_preserves_both_sources_and_allows_retry() -> Check {
    let text = "A<<!--note-->b>B";
    let (_root, source, work) = work(text)?;
    let sheet = "worksheets/s0001.json";
    let row = |action, body, note| {
        let mut row = edit(text, body);
        row["action"] = json!(action);
        row["structure_note"] = json!(note);
        json!([row])
    };
    files::write_json(
        &work.join(sheet),
        &row("preserve", text, "Cannot authorize activation"),
    )?;
    err(workspace::build(&work, None), "activat");
    assert!(!work.join("builds").exists());
    files::write_json(
        &work.join(sheet),
        &row(
            "edit",
            "A less than b greater than B",
            "Explicit literal adaptation",
        ),
    )?;
    assert_eq!(workspace::build(&work, None)?["episodes"][0]["cues"], 1);
    for path in [source, work.join("sources/s0001.srt")] {
        assert_eq!(
            fs::read(path).map_err(|e| e.to_string())?,
            srt(text).as_bytes()
        );
    }
    Ok(())
}

#[test]
fn fingerprint_retains_unicode_basename_size_and_sha256() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = root.path().join("stable فارسی.bin");
    let data = "سلام OVA\n".as_bytes();
    fs::write(&path, data).map_err(|e| e.to_string())?;
    assert_eq!(
        files::file_fingerprint(&path, data.len() as u64)?,
        json!({
            "name":"stable فارسی.bin","bytes":data.len(),"sha256":"cc5989decbf7065044ab18992e1718d755ec702d3eae3a522e5dee7ee833bbc1",
        })
    );
    Ok(())
}

#[test]
fn fingerprint_caps_are_checked_before_open() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = root.path().join("input.bin");
    err(
        runtime::file_fingerprint(path_marker(&path), Some(-1)),
        "limit",
    );
    fs::write(&path, b"AB").map_err(|e| e.to_string())?;
    err(files::file_fingerprint(&path, 1), "limit");
    err(files::file_fingerprint(root.path(), 128), "regular");
    Ok(())
}

#[test]
fn same_length_replacement_has_different_fingerprint() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = root.path().join("declared.bin");
    let alternate = root.path().join("alternate.bin");
    fs::write(&path, b"original").map_err(|e| e.to_string())?;
    fs::write(&alternate, b"replaced").map_err(|e| e.to_string())?;
    let first = files::file_fingerprint(&path, 8)?;
    let other = files::file_fingerprint(&alternate, 8)?;
    assert_eq!(first["bytes"], other["bytes"]);
    assert_ne!(first["sha256"], other["sha256"]);
    assert_eq!(fs::read(path).map_err(|e| e.to_string())?, b"original");
    assert_eq!(fs::read(alternate).map_err(|e| e.to_string())?, b"replaced");
    Ok(())
}

macro_rules! invalid_json {
    ($($name:ident: $text:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let root = tempfile::tempdir().map_err(|e| e.to_string())?;
            let path = root.path().join("metadata.json");
            fs::write(&path, $text).map_err(|e| e.to_string())?;
            err(files::read_json(&path, 1024), "metadata.json");
            Ok(())
        }
    )+};
}

invalid_json! {
    duplicate_json_fields: r#"{"x":1,"x":2}"#,
    nan_json_number: r#"{"x":NaN}"#,
    infinite_json_number: r#"{"x":Infinity}"#,
    overflow_json_number: r#"{"x":1e9999}"#,
    truncated_json_value: r#"{"x":"#,
}

macro_rules! bad_research {
    ($($name:ident: $url:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() {
            let input = json!({"series":"Authored","terms_reviewed":true,"terms":[],"research":[{"url":$url,"note":"Fixture"}]});
            err(validation::glossary(input, "glossary.json".into()), "research");
        }
    )+};
}

bad_research! {
    scheme_only_research: "https:",
    empty_host_research: "https://",
    credentialed_research: "https://u:p@example.org",
    invalid_port_research: "https://example.org:99999",
    space_in_research_host: "https://bad host",
}
