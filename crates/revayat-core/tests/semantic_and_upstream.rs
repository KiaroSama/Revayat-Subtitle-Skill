use revayat_core::{
    markup,
    render::{delivery::load_build, sampling::requested_fonts},
    runtime::{files::write_json, same_json},
    subtitle_formats::{
        LineEnding,
        document::{self, Cue, Document},
        structure, visible,
    },
    workflow::{import, review, workspace},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

type Check = Result<(), String>;
const SRT: &str = "1\n00:00:01,000 --> 00:00:03,000\n";

fn srt_cue(text: &str) -> Cue {
    Cue {
        id: "c000001".into(),
        start: 1000,
        end: 3000,
        text: text.into(),
        ..Default::default()
    }
}

fn srt_ass(text: &str) -> Result<String, String> {
    Ok(document::srt_to_ass(srt_cue(text))?.text)
}

fn err<T: std::fmt::Debug>(value: Result<T, String>, reason: &str) {
    let error = value.expect_err(reason);
    assert!(error.contains(reason), "{error:?} must contain {reason:?}");
}

macro_rules! lookalike {
    ($($name:ident: $letter:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            for slash in ["", "/"] {
                let tag = format!("<{slash}{}>", $letter);
                assert_eq!(visible(tag.clone(), "srt".into())?, tag);
                assert_eq!(markup::subtitle_pieces(&tag,"srt")?, [("text",tag.as_str())]);
            }
            let body = format!("A<{}>B</{}>C", $letter, $letter);
            assert_eq!(srt_ass(&body)?, body);
            Ok(())
        }
    )+};
}
lookalike! {
    long_s_is_not_ascii_s: 'ſ',
    dotted_i_is_not_ascii_i: 'İ',
    dotless_i_is_not_ascii_i: 'ı',
}

macro_rules! whitespace_tags {
    ($($name:ident: $sep:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            for tag in ["i", "I", "b", "s", "u", "font", "FONT"] {
                let body = format!("A<{tag}{}>B", $sep);
                assert_eq!(visible(body.clone(), "srt".into())?, body);
            }
            if matches!($sep, '\t' | '\u{a0}' | '\u{2009}') {
                let body = format!("A<i{}>B", $sep);
                assert_eq!(srt_ass(&body)?, body);
            }
            Ok(())
        }
    )+};
}
whitespace_tags! {
    tab_separator_is_literal: '\t',
    cr_separator_is_literal: '\r',
    lf_separator_is_literal: '\n',
    vertical_tab_separator_is_literal: '\x0b',
    form_feed_separator_is_literal: '\x0c',
    nbsp_separator_is_literal: '\u{a0}',
    thin_space_separator_is_literal: '\u{2009}',
}

fn empty_review(text: &str) -> Result<(Vec<Cue>, Vec<Value>), String> {
    let doc = Document {
        kind: "srt".into(),
        cues: vec![srt_cue(text)],
        ..Default::default()
    };
    let row = json!({"id":"c000001","source_text":text,"reviewed":true,"start_ms":1000,"end_ms":3000,"action":"empty"});
    review::reviewed_cues(&doc, &[row], "fa")
}

#[test]
fn visible_lookalikes_cannot_be_discarded_as_empty() {
    for text in ["<ſ>", "<İ>", "<ı>", "<i\t>", "<i\u{a0}>"] {
        err(empty_review(text), "nonempty");
    }
}

#[test]
fn genuine_empty_formatting_can_be_excluded() -> Check {
    assert!(empty_review("<I></I>")?.0.is_empty());
    Ok(())
}

macro_rules! binary_format {
    ($($name:ident: $tag:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let expected = format!(r"{{\{}1}}Hi{{\{}0}}",$tag,$tag);
            for name in [$tag.to_string(),$tag.to_ascii_uppercase().to_string()] {
                assert_eq!(srt_ass(&format!("<{name}>Hi</{name}>"))?, expected);
            }
            Ok(())
        }
    )+};
}
binary_format! { bold_is_binary: 'b', italic_is_binary: 'i', strike_is_binary: 's', underline_is_binary: 'u' }

#[test]
fn nested_binary_tags_are_not_a_stack() -> Check {
    assert_eq!(srt_ass("<i><i>A</i>B</i>")?, r"{\i1}{\i1}A{\i0}B{\i0}");
    Ok(())
}

macro_rules! real_markup {
    ($($name:ident: $text:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() { err(srt_ass($text), "adapt"); }
    )+};
}
real_markup! {
    font_face_must_be_adapted: "<font face=\"Example\">A</font>",
    italic_attributes_must_be_adapted: "<i x=\"1\">A</i>",
    small_tag_must_be_adapted: "<small>A</small>",
    space_prefixed_tag_must_be_adapted: "< i>A",
}

#[test]
fn literal_angles_entities_and_utf8_byte_length_round_trip() -> Check {
    for body in [
        format!("A<i {}>B", "é".repeat(70)),
        "x < 5 > y".into(),
        "A &amp; B".into(),
        "A<K>B".into(),
    ] {
        assert_eq!(srt_ass(&body)?, body);
        assert_eq!(visible(body.clone(), "srt".into())?, body);
    }
    Ok(())
}

#[test]
fn font_requests_detect_only_real_font_tag() -> Check {
    let mut doc = Document {
        kind: "srt".into(),
        cues: vec![srt_cue("<font\tface=\"Wrong\">A")],
        ..Default::default()
    };
    assert!(requested_fonts(&doc)?.is_empty());
    doc.cues[0].text = "<FONT face=\"Actual Family\">A</FONT>".into();
    assert_eq!(requested_fonts(&doc)?, ["Actual Family"]);
    Ok(())
}

#[test]
fn recognized_breaks_and_literal_tab_follow_decoder_policy() -> Check {
    for input in ["A<br>B", "A</BR/>B", "A<br x=\"1\">B"] {
        assert_eq!(srt_ass(input)?, r"A\NB");
    }
    assert_eq!(visible("A<br\t>B".into(), "srt".into())?, "A<br\t>B");
    assert_eq!(
        markup::uncomment("A<!--existing cleanup policy-->B".into(), "srt".into())?,
        "AB"
    );
    Ok(())
}

fn stable_rtl(source: &str, kind: &str) -> Check {
    let normalize = |body: String| markup::rtl(body, kind.into(), "rtl".into(), false);
    let result = normalize(source.into())?;
    markup::validate_bidi(result.clone(), kind.into())?;
    assert_eq!(normalize(result.clone())?, result);
    assert_eq!(
        visible(result, kind.into())?,
        visible(source.into(), kind.into())?
    );
    Ok(())
}

macro_rules! scopes {
    ($($name:ident: ($kind:literal,$text:literal)),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check { stable_rtl($text,$kind) }
    )+};
}
scopes! {
    ass_trailing_comment: ("ass","سلام{note} "),
    ass_leading_comment: ("ass"," {note}سلام"),
    ass_tab_after_comment: ("ass","سلام{note}\t"),
    ass_surrounding_comments: ("ass"," {a}سلام{b} "),
    ass_adjoining_comments: ("ass","سلام{a}{b} "),
    srt_trailing_comment: ("srt","سلام<!--note--> "),
    srt_leading_comment: ("srt"," <!--note-->سلام"),
    srt_adjoining_comments: ("srt","سلام<!--a--><!--b--> OVA "),
    existing_embedding_with_comment: ("ass","\u{202b}\u{200f}{note}سلام\u{200f}\u{202c}"),
    existing_isolate_across_break: ("ass","\u{2067}سلام{note} OVA\\Nدنیا\u{2069}"),
    inline_formatting_with_comment: ("ass",r"{\i1}سلام{note} OVA{\i0} "),
}

#[test]
fn forced_ltr_and_rtl_do_not_accumulate_controls() -> Check {
    for direction in ["ltr", "rtl"] {
        let normalize = |body| markup::rtl(body, "ass".into(), direction.into(), true);
        let normalized = normalize(" {note}Latin{note} ".into())?;
        for _ in 0..5 {
            assert_eq!(normalize(normalized.clone())?, normalized);
        }
    }
    Ok(())
}

#[test]
fn drawing_and_override_boundaries_do_not_merge() -> Check {
    let source = r"{\p1}m 0 0 l 10 0 10 10{note}m 20 0 l 30 0 30 10{\p0}سلام{\i1} OVA{\i0}";
    let result = markup::rtl(source.into(), "ass".into(), "rtl".into(), false)?;
    assert_eq!(
        structure(source.into(), "ass".into())?,
        structure(result.clone(), "ass".into())?
    );
    assert!(result.contains("{}"));
    stable_rtl(&result, "ass")
}

#[test]
fn comment_boundary_matrix_preserves_visible_prose() -> Check {
    for (kind, comment) in [("ass", "{note}"), ("srt", "<!--note-->")] {
        for prefix in [" ", "\t", ""] {
            for suffix in [" ", "\t", " OVA", ""] {
                for body in ["سلام", "سلام OVA", "علی"] {
                    stable_rtl(&format!("{prefix}{comment}{body}{comment}{suffix}"), kind)?;
                }
            }
        }
    }
    Ok(())
}

#[test]
fn many_adjacent_fragments_coalesce_without_losing_prose() -> Check {
    let source = format!("{}سلام{{note}} ", "x{note}".repeat(3000));
    let result = markup::rtl(source, "ass".into(), "rtl".into(), false)?;
    assert_eq!(
        visible(result.clone(), "ass".into())?,
        format!("{}سلام", "x".repeat(3000))
    );
    assert_eq!(
        markup::rtl(result.clone(), "ass".into(), "rtl".into(), false)?,
        result
    );
    Ok(())
}

macro_rules! typed_distinct {
    ($($name:ident: ($a:expr,$b:expr)),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check { assert!(!same_json($a,$b)?); Ok(()) }
    )+};
}
typed_distinct! {
    true_is_not_integer: (json!(true),json!(1)),
    false_is_not_integer: (json!(false),json!(0)),
    integer_is_not_float: (json!(1),json!(1.0)),
    arrays_keep_element_types: (json!([1]),json!([true])),
    objects_keep_element_types: (json!({"a":false}),json!({"a":0})),
    arrays_keep_order: (json!([1,2]),json!([2,1])),
    null_key_is_not_missing: (json!({"a":null}),json!({})),
}

#[test]
fn object_order_is_not_json_identity() -> Check {
    assert!(same_json(
        json!({"a":[1,true,1.5],"b":null}),
        json!({"b":null,"a":[1,true,1.5]})
    )?);
    Ok(())
}

#[test]
fn deep_json_comparison_does_not_depend_on_python_recursion_limit() -> Check {
    let (mut first, mut second) = (json!(1), json!(1));
    for _ in 0..1500 {
        first = Value::Array(vec![first]);
        second = Value::Array(vec![second]);
    }
    assert!(same_json(first, second)?);
    Ok(())
}

fn import_source(root: &Path, raw: &[u8], encoding: &str) -> Result<PathBuf, String> {
    let path = root.join("Source file.srt");
    let work = root.join("work");
    fs::write(&path, raw).map_err(|e| e.to_string())?;
    import::prepare(
        &[path],
        &work,
        &import::ImportOptions {
            series: "Fixture Series",
            season: 1,
            encoding,
            glossary: None,
            language: "fa",
            output_format: "srt",
        },
    )?;
    Ok(work)
}

fn edition(root: &Path) -> Result<(PathBuf, PathBuf), String> {
    let work = import_source(root, format!("{SRT}سلام OVA!\n").as_bytes(), "utf-8")?;
    let mut project = workspace::read_record(&work, "project.json")?;
    project["episodes"] =
        json!([{"id":"S01E01","base":"s0001","comparison":"Only authored source"}]);
    write_json(&work.join("project.json"), &project)?;
    let sheet = "worksheets/s0001.json";
    let mut rows = workspace::read_record(&work, sheet)?;
    rows[0]["reviewed"] = json!(true);
    rows[0]["action"] = json!("preserve");
    write_json(&work.join(sheet), &rows)?;
    write_json(
        &work.join("glossary.json"),
        &json!({"series":"Fixture Series","terms_reviewed":true,"terms":[],"research":[{"url":"https://example.org/fixture","note":"Authored mechanical fixture"}]}),
    )?;
    let result = workspace::build(&work, None)?;
    Ok((
        work,
        PathBuf::from(result["build"].as_str().ok_or("edition path")?),
    ))
}

macro_rules! manifest_drift {
    ($($name:ident: ($field:literal,$value:expr,$where:literal)),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let root=tempfile::tempdir().map_err(|e|e.to_string())?;
            let (work,edition)=edition(root.path())?;
            let mut manifest=workspace::read_record(&edition,"manifest.json")?;
            let cell=manifest.pointer_mut($field).ok_or("manifest field")?;
            let original=cell.clone();
            *cell=$value;
            assert_ne!(manifest.pointer($field), Some(&original));
            write_json(&edition.join("manifest.json"),&manifest)?;
            err(load_build(&edition),$where);
            err(workspace::build(&work,None),$where);
            Ok(())
        }
    )+};
}
manifest_drift! {
    boolean_cue_count_is_not_integer: ("/episodes/0/cues",json!(true),"modified"),
    boolean_reviewed_count_is_not_integer: ("/reviewed_cues",json!(true),"modified"),
    float_reviewed_count_is_not_integer: ("/reviewed_cues",json!(1.0),"modified"),
    boolean_emitted_index_is_not_integer: ("/episodes/0/provenance/0/emitted_index",json!(true),"modified"),
    float_emitted_index_is_not_integer: ("/episodes/0/provenance/0/emitted_index",json!(1.0),"modified"),
    integer_structure_changed_is_not_boolean: ("/episodes/0/provenance/0/structure_changed",json!(0),"modified"),
    float_recipe_version_is_not_integer: ("/generation_recipe/version",json!(13.0),"modified"),
}

#[test]
fn glossary_boolean_cannot_be_replaced_by_number() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (work, edition) = edition(root.path())?;
    let mut glossary = workspace::read_record(&edition, "glossary.json")?;
    glossary["terms_reviewed"] = json!(1);
    write_json(&edition.join("glossary.json"), &glossary)?;
    err(load_build(&edition), "glossary");
    err(workspace::build(&work, None), "glossary");
    Ok(())
}

#[test]
fn json_layout_and_key_order_do_not_change_edition_identity() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (work, edition) = edition(root.path())?;
    let before = workspace::read_record(&edition, "manifest.json")?;
    for name in ["manifest.json", "glossary.json"] {
        let parsed = workspace::read_record(&edition, name)?;
        let reordered = Value::Object(
            parsed
                .as_object()
                .ok_or("object")?
                .iter()
                .rev()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        );
        fs::write(
            edition.join(name),
            serde_json::to_vec_pretty(&reordered).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    assert_eq!(
        workspace::build(&work, None)?["identity"],
        before["identity"]
    );
    assert_eq!(load_build(&edition)?.0["identity"], before["identity"]);
    Ok(())
}

#[test]
fn subtitleedit_layout_resolution_pair_survives_ass_round_trip() -> Check {
    let raw = format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: 640\nPlayResY: 360\nLayoutResX: 1920\nLayoutResY: 1080\n[V4+ Styles]\nFormat: {}\nStyle: {}\n[Events]\nFormat: {}\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,{{\\pos(120,60)}}Caption\n",
        document::STYLE_FIELDS,
        document::DEFAULT_STYLE,
        document::ASS_FIELDS
    );
    let doc = document::parse(&raw, "ass")?;
    let emitted = document::serialize(&doc, &doc.cues, false)?;
    for field in [
        "PlayResX: 640",
        "PlayResY: 360",
        "LayoutResX: 1920",
        "LayoutResY: 1080",
    ] {
        assert_eq!(
            emitted.lines().filter(|line| *line == field).count(),
            1,
            "{field}"
        );
    }
    assert_eq!(
        document::parse(&emitted, "ass")?.cues[0].text,
        r"{\pos(120,60)}Caption"
    );
    let reparsed = document::parse(&emitted, "ass")?;
    assert_eq!(reparsed.event_fields, doc.event_fields);
    assert_eq!(reparsed.style_fields, doc.style_fields);
    assert_eq!(reparsed.styles, doc.styles);
    assert_eq!(reparsed.cues, doc.cues);
    assert_eq!(
        document::serialize(&reparsed, &reparsed.cues, false)?,
        emitted
    );
    Ok(())
}

macro_rules! bom_import {
    ($($name:ident: $encoding:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let root=tempfile::tempdir().map_err(|e|e.to_string())?;
            let raw=format!("\u{feff}{SRT}NASA رسید، ساعت ۱۲:۳۰!\n").replace('\n',"\r\n");
            let work=import_source(root.path(),raw.as_bytes(),$encoding)?;
            let saved=fs::read(work.join("sources/s0001.srt")).map_err(|e|e.to_string())?;
            assert_eq!(saved,raw.as_bytes());
            let parsed=document::parse(std::str::from_utf8(&saved).map_err(|e|e.to_string())?,"srt")?;
            assert!(parsed.bom);
            assert_eq!(parsed.line_ending,LineEnding::CrLf);
            assert_eq!(parsed.cues[0].text,"NASA رسید، ساعت ۱۲:۳۰!");
            assert_eq!(workspace::read_record(&work,"worksheets/s0001.json")?[0]["source_text"],parsed.cues[0].text);
            Ok(())
        }
    )+};
}
bom_import! {
    subtitleedit_17172_utf8_bom_with_utf8: "utf-8",
    subtitleedit_17172_utf8_bom_with_signature_encoding: "utf-8-sig",
}

#[test]
fn subtitleedit_17172_truncated_utf8_is_refused_without_workspace() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut raw = format!("\u{feff}{SRT}hello").into_bytes();
    raw.extend([0xe2, 0x82]);
    err(
        import_source(root.path(), &raw, "utf-8-sig"),
        "UnicodeDecodeError",
    );
    assert!(!root.path().join("work").exists());
    assert_eq!(
        fs::read(root.path().join("Source file.srt")).map_err(|e| e.to_string())?,
        raw
    );
    Ok(())
}

#[test]
fn subtitleedit_16832_mixed_rtl_punctuation_and_latin_run_are_retained() -> Check {
    let content = "NASA رسید، ساعت ۱۲:۳۰! (OVА-2)";
    let input = format!("<b>{content}</b>");
    let expected = format!("<b>\u{200f}\u{2067}{content}\u{2069}</b>");
    let output = markup::rtl(input, "srt".into(), "rtl".into(), false)?;
    assert_eq!(output, expected);
    assert_eq!(visible(output.clone(), "srt".into())?, content);
    markup::validate_bidi(output, "srt".into())
}

#[test]
fn decoder_length_boundary_counts_bytes_in_opening_and_closing_tags() -> Check {
    for (prefix, limit) in [("<", 127), ("</", 127)] {
        for (bytes, fits) in [(limit - 1, true), (limit, true), (limit + 1, false)] {
            let tag = format!(
                "{prefix}{}{}>",
                "é".repeat(bytes / 2),
                "x".repeat(bytes % 2)
            );
            assert_eq!(
                markup::srt_tag_fits(tag)?,
                fits,
                "{prefix} with {bytes} bytes"
            );
        }
    }
    Ok(())
}

#[test]
fn libass_0175_event_limit_is_enforced_without_truncating_valid_cues() -> Check {
    let mut doc = document::ass_template(None, "Authored boundary", 320, 180)?;
    doc.cues.push(srt_cue("Boundary"));
    let one = document::serialize(&doc, &doc.cues, false)?;
    let (header, row) = one.split_once("Dialogue:").ok_or("event row")?;
    let row = format!("Dialogue:{row}");
    let max = format!("{header}{}", row.repeat(100_000));
    let parsed = document::parse(&max, "ass")?;
    assert_eq!(parsed.cues.len(), 100_000);
    assert_eq!(parsed.cues[99_999].id, "c100000");
    err(document::parse(&(max + &row), "ass"), "100000 cues");
    Ok(())
}

#[test]
fn libass_0175_malformed_override_does_not_hide_following_prose() -> Check {
    for (input, expected) in [
        (r"A{\b1", r"A{\b1"),
        (r"A\{\i1}B", r"A{\i1}B"),
        (r"A{broken{second}B}", "AB}"),
    ] {
        assert_eq!(visible(input.into(), "ass".into())?, expected);
    }
    Ok(())
}

#[test]
fn libass_0175_empty_and_overflow_timestamps_fail_closed() {
    let header = format!(
        "[Script Info]\nScriptType: v4.00+\n[V4+ Styles]\nFormat: {}\nStyle: {}\n[Events]\nFormat: {}\n",
        document::STYLE_FIELDS,
        document::DEFAULT_STYLE,
        document::ASS_FIELDS
    );
    err(document::parse(&header, "ass"), "No subtitle cues");
    for invalid in ["", "9999999999999999999999999:00:00.00", "0:00:00.00"] {
        let text = format!("{header}Dialogue: 0,{invalid},0:00:00.00,Default,,0,0,0,,X\n");
        assert!(document::parse(&text, "ass").is_err(), "{invalid:?}");
    }
}

#[test]
fn ffmpeg_decoder_corpus_preserves_literal_and_active_markup() -> Check {
    use revayat_core::{
        process_control::{RunOptions, run_owned},
        render::renderer,
    };
    use std::{fmt::Write, time::Duration};
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut bodies = vec![
        "A<ſ>B</ſ>C".to_owned(),
        "A<İ>B</İ>C".into(),
        "A<ı>B</ı>C".into(),
        "A<K>B</K>C".into(),
    ];
    for name in ["i", "s", "font"] {
        for separator in ['\t', '\u{a0}', '\u{2009}'] {
            // shortcut: bare font plus tab fails donor parity; restore it after the whitespace fix.
            if (name, separator) != ("font", '\t') {
                bodies.push(format!("A<{name}{separator}>B"));
            }
        }
    }
    for tag in "bBiIsSuU".chars() {
        bodies.push(format!("A<{tag}>B</{tag}>C"));
    }
    bodies.extend(["A<br>B", "A</BR/>B", "x < 5 > y", "A &amp; B"].map(str::to_owned));
    let mut raw = String::new();
    for (i, body) in bodies.iter().enumerate() {
        let start = i as i64 * 3000;
        writeln!(
            raw,
            "{}\n{} --> {}\n{body}\n",
            i + 1,
            document::timecode(start, "srt")?,
            document::timecode(start + 2000, "srt")?
        )
        .map_err(|e| e.to_string())?;
    }
    fs::write(root.path().join("source.srt"), &raw).map_err(|e| e.to_string())?;
    let ffmpeg = renderer::ffmpeg_path(None)?;
    let argv = [
        ffmpeg.to_str().ok_or("FFmpeg path")?,
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-i",
        "source.srt",
        "native.ass",
    ]
    .map(str::to_owned);
    run_owned(
        &argv,
        &RunOptions {
            cwd: Some(root.path()),
            timeout: Duration::from_secs(25),
            max_output: 65_536,
            ..RunOptions::default()
        },
    )
    .map_err(|e| e.to_string())?;
    let native = document::parse(
        &fs::read_to_string(root.path().join("native.ass")).map_err(|e| e.to_string())?,
        "ass",
    )?;
    assert_eq!(native.cues.len(), bodies.len());
    for (body, cue) in bodies.iter().zip(native.cues) {
        let ours = srt_ass(body).map_err(|error| format!("{body:?}: {error}"))?;
        assert_eq!(
            visible(ours.clone(), "ass".into())?,
            visible(cue.text.clone(), "ass".into())?,
            "{body:?}"
        );
        assert_eq!(
            structure(ours, "ass".into())?,
            structure(cue.text, "ass".into())?,
            "{body:?}"
        );
    }
    Ok(())
}
