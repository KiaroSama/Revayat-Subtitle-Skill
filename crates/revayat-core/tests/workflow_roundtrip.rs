use revayat_core::{
    markup,
    publication::{publish_bytes, rename_noreplace},
    render::{delivery, sampling},
    runtime::{digest_bytes, files},
    subtitle_formats::{
        document::{self, Cue, Document},
        visible,
    },
    workflow::{
        import::{self, ImportOptions},
        review, workspace,
    },
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use zip::{ZipWriter, write::SimpleFileOptions};

type Check = Result<(), String>;
const SOURCE: &str = "1\n00:00:01,000 --> 00:00:03,000\nPlain dialogue\n";

fn cue(body: &str) -> Cue {
    Cue {
        id: "c000001".into(),
        start: 1000,
        end: 3000,
        text: body.into(),
        ..Cue::default()
    }
}
fn srt_ass(body: &str) -> Result<String, String> {
    Ok(document::srt_to_ass(cue(body))?.text)
}
fn err<T: std::fmt::Debug>(result: Result<T, String>, expected: &str) {
    let error = result.expect_err(expected);
    assert!(
        error.contains(expected),
        "{error:?} must contain {expected:?}"
    );
}
fn write(path: &Path, data: impl AsRef<[u8]>) -> Check {
    fs::write(path, data).map_err(|e| e.to_string())
}

macro_rules! unknown_markup {
    ($($name:ident: $tag:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() { err(srt_ass(&format!("A<{}>B</{}>C",$tag,$tag)),"markup"); }
    )+};
}
unknown_markup! {
    unknown_small_tag: "small", unknown_foo_tag: "foo", unknown_numeric_tag: "3", unknown_break_tag: "break",
    unknown_ruby_tag: "ruby", unknown_span_tag: "span", unknown_underscore_tag: "_name", unknown_slash_tag: "foo/bar",
}

macro_rules! reject_markup {
    ($($name:ident: ($body:literal, $reason:literal)),+ $(,)?) => {$ (
        #[test]
        fn $name() { err(srt_ass($body),$reason); }
    )+};
}
reject_markup! {
    unknown_close: ("A</unknown>B","markup"),
    attributed_italic: ("A<i class=\"x\">B</i>C","markup"),
    space_prefixed_bold: ("A< b>B</b>","markup"),
    unsupported_font_face: ("A<font face=\"Arial\">B</font>","markup"),
    unsupported_named_color: ("A<font color=\"red\">B</font>","markup"),
    tab_prefixed_font_attribute: ("A<font\tface=\"Arial\">B</font>","markup"),
    empty_decoder_tag: ("A<>B","markup"),
    empty_decoder_closing_tag: ("A</>B","markup"),
    empty_decoder_doubled_slash: ("A<//>B","markup"),
    unclosed_comment_cannot_hide_ass_control: (r"A<!-- unfinished {\b1}","control syntax"),
    literal_ass_bold_is_not_safe_in_srt_donor: (r"{\b1}A","control syntax"),
    literal_ass_break_is_not_safe_in_srt_donor: (r"A\N B","control syntax"),
}

macro_rules! closing_breaks {
    ($($name:ident: $tag:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let body=format!("A{}B",$tag);
            assert_eq!(srt_ass(&body)?,r"A\NB");
            assert_eq!(visible(body,"srt".into())?,"A\nB");
            Ok(())
        }
    )+};
}
closing_breaks! {
    plain_closing_break: "</br>", uppercase_closing_break: "</BR>", self_closing_close_break: "</br/>",
    space_closing_break: "</br >", slash_space_closing_break: "</br/ >", attributed_closing_break: "</br a=\"b\">",
}

#[test]
fn closing_break_anchors_two_persian_display_lines() -> Check {
    let rtl = |value| markup::rtl(value, "srt".into(), "rtl".into(), false);
    let normalized = rtl("سلام</br>دنیا OVA".into())?;
    assert_eq!(normalized.matches("\u{200f}\u{2067}").count(), 2);
    assert_eq!(visible(normalized.clone(), "srt".into())?, "سلام\nدنیا OVA");
    assert_eq!(rtl(normalized.clone())?, normalized);
    Ok(())
}

macro_rules! literal_markup {
    ($($name:ident: $body:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check { assert_eq!(srt_ass($body)?,$body); Ok(()) }
    )+};
}
literal_markup! {
    ascii_comparison: "x < 5 > y", persian_comparison: "x <۵> y", nested_angle: "A<<foo>>B",
    numeric_and_named_entities: "&amp; &#65;", tab_after_angle: "A<\tb>B", tab_after_break_name: "A<br\t>B",
    unicode_space_after_break: "A<br\u{a0}>B", incomplete_tag: "A<unfinished", nested_empty_tag: "A<<>>B",
    unfinished_comment: "A<!-- unfinished",
}

#[test]
fn formatting_is_binary_rather_than_stacked() -> Check {
    assert_eq!(
        srt_ass("A<b>B<b>C</b>D</b>E")?,
        r"A{\b1}B{\b1}C{\b0}D{\b0}E"
    );
    for tag in ['i', 'u', 's'] {
        assert_eq!(
            srt_ass(&format!("<{tag}>A</{tag}>"))?,
            format!(r"{{\{tag}1}}A{{\{tag}0}}")
        );
    }
    Ok(())
}

#[test]
fn completed_comments_can_contain_newlines_and_controls() -> Check {
    assert_eq!(srt_ass("A<!-- hidden\ncomment -->B")?, "AB");
    assert_eq!(srt_ass(r"A<!-- {\b1} hidden \N -->B")?, "AB");
    Ok(())
}

#[test]
fn decoder_breaks_at_byte_boundary_are_recognized_only_within_limit() -> Check {
    for prefix in ["<br ", "</br ", "<br/ "] {
        for attributes in ["x".repeat(128), "ی".repeat(64)] {
            let text = format!("A{prefix}{attributes}>B");
            assert_eq!(srt_ass(&text)?, text);
            assert_eq!(visible(text.clone(), "srt".into())?, text);
        }
    }
    assert_eq!(srt_ass(&format!("A<br {}>B", "x".repeat(124)))?, r"A\NB");
    Ok(())
}

#[test]
fn visibility_does_not_adopt_unknown_decoder_tags() -> Check {
    for body in ["A<small>B</small>C", "A<break>B"] {
        assert_eq!(visible(body.into(), "srt".into())?, body);
    }
    Ok(())
}

#[test]
fn unsupported_donor_refusal_preserves_caller_cue() {
    let input = Cue {
        start: 1001,
        end: 3009,
        ..cue("A<small>B</small>C")
    };
    let before = input.clone();
    err(document::srt_to_ass(input.clone()), "markup");
    assert_eq!(input, before);
}

#[test]
fn srt_donor_floors_both_timestamps_and_collapsed_timing_is_rejected() -> Check {
    let c = document::srt_to_ass(Cue {
        start: 1001,
        end: 3009,
        ..cue("A\r\nB")
    })?;
    assert_eq!((c.start, c.end, c.text.as_str()), (1000, 3000, r"A\NB"));
    err(
        document::srt_to_ass(Cue {
            start: 1001,
            end: 1009,
            ..cue("A")
        }),
        "collapses",
    );
    Ok(())
}

#[test]
fn unterminated_break_whitespace_remains_intact() -> Check {
    for sep in [" ", "\t", "\u{a0}"] {
        let body = format!("A<br{}", sep.repeat(50_000));
        assert_eq!(srt_ass(&body)?, body);
    }
    Ok(())
}

#[test]
fn break_dialect_accepts_ascii_space_attributes_only() -> Check {
    for (tag, expected) in [
        ("<br x=\"1\">", r"A\NB"),
        ("</br x=\"1\">", r"A\NB"),
        ("<br\tx=\"1\">", "A<br\tx=\"1\">B"),
        ("<br\u{a0}x=\"1\">", "A<br\u{a0}x=\"1\">B"),
    ] {
        assert_eq!(srt_ass(&format!("A{tag}B"))?, expected);
    }
    Ok(())
}

#[test]
fn default_samples_cover_middle_closing_break() -> Check {
    let doc = Document {
        kind: "srt".into(),
        cues: vec![
            cue("A"),
            Cue {
                start: 3000,
                end: 4000,
                ..cue("B</br>C")
            },
            Cue {
                start: 5000,
                end: 6000,
                ..cue("D")
            },
        ],
        ..Document::default()
    };
    assert!(
        sampling::samples(&doc, false, &[])?
            .iter()
            .any(|sample| sample["time_seconds"] == 3.5)
    );
    Ok(())
}

fn ass(text: &str) -> Result<Document, String> {
    let mut doc = document::ass_template(None, "Fixture", 1920, 1080)?;
    let mut cue = cue(text);
    cue.fields.insert("style".into(), "Default".into());
    doc.cues.push(cue);
    Ok(doc)
}

fn add_style(doc: &mut Document, name: &str) {
    let mut row = doc.styles["Default"].clone();
    row[0] = name.into();
    doc.styles.insert(name.into(), row);
}

#[test]
fn unsupported_srt_donor_does_not_add_default_style() -> Check {
    let mut base = ass("Base")?;
    add_style(&mut base, "Base");
    base.styles.shift_remove("Default");
    base.cues[0].fields.insert("style".into(), "Base".into());
    let before = base.clone();
    let donor = document::parse(
        &SOURCE.replace("Plain dialogue", "<font face=\"Arial\">A</font>"),
        "srt",
    )?;
    err(
        review::donor(&mut base, &donor, donor.cues.clone(), "s0002", true),
        "markup",
    );
    assert_eq!(base, before);
    assert!(!base.styles.contains_key("Default"));
    Ok(())
}

#[test]
fn undefined_donor_reset_keeps_both_inputs_unchanged() -> Check {
    let mut base = ass("Base")?;
    let donor = ass(r"{\rMissing}Hi")?;
    let before = (base.clone(), donor.clone());
    err(
        review::donor(&mut base, &donor, donor.cues.clone(), "s0002", true),
        "undefined style",
    );
    assert_eq!((base, donor), before);
    Ok(())
}

#[test]
fn conflicting_donor_fonts_do_not_leave_namespaced_styles() -> Check {
    let mut base = ass("Base")?;
    let mut donor = ass("Donor")?;
    base.sections.insert(
        2,
        (
            "[Fonts]".into(),
            vec!["fontname: fixture.ttf".into(), "!!!!".into()],
        ),
    );
    donor.sections.insert(
        2,
        (
            "[Fonts]".into(),
            vec!["fontname: fixture.ttf".into(), "####".into()],
        ),
    );
    let before = base.clone();
    err(
        review::donor(&mut base, &donor, donor.cues.clone(), "s0002", true),
        "font sets",
    );
    assert_eq!(base, before);
    Ok(())
}

#[test]
fn successful_donor_remaps_reset_and_preserves_original_cue() -> Check {
    let mut base = ass("Base")?;
    let donor = ass(r"{\r(Default)}Hi")?;
    let before = donor.clone();
    let merged = review::donor(&mut base, &donor, donor.cues.clone(), "s0002", true)?;
    assert_eq!(merged[0].text, r"{\r(s0002__Default)}Hi");
    assert_eq!(merged[0].fields["style"], "s0002__Default");
    assert_eq!(donor, before);
    Ok(())
}

#[test]
fn failed_late_style_collision_does_not_mutate_base() -> Check {
    let mut base = ass("Base")?;
    let mut donor = ass("Donor")?;
    add_style(&mut donor, "Other");
    add_style(&mut base, "s0002__Other");
    let before = base.clone();
    err(
        review::donor(&mut base, &donor, donor.cues.clone(), "s0002", true),
        "collision",
    );
    assert_eq!(base, before);
    assert!(!base.styles.contains_key("s0002__Default"));
    Ok(())
}

fn prepare(inputs: &[PathBuf], work: &Path) -> Result<Value, String> {
    import::prepare(
        inputs,
        work,
        &ImportOptions {
            series: "Fixture",
            season: 1,
            encoding: "utf-8",
            glossary: None,
            language: "en",
            output_format: "ass",
        },
    )
}
fn reviewed(root: &Path, bodies: &[&str], alternates: bool) -> Result<PathBuf, String> {
    let mut inputs = Vec::new();
    for (i, body) in bodies.iter().enumerate() {
        let path = root.join(format!("input {i}.srt"));
        write(&path, body)?;
        inputs.push(path);
    }
    let work = root.join("work");
    prepare(&inputs, &work)?;
    let mut metadata = workspace::read_record(&work, "project.json")?;
    let mut episodes = Vec::new();
    for (i, source) in metadata["sources"]
        .as_array()
        .ok_or("sources")?
        .iter()
        .enumerate()
    {
        let id = source["id"].as_str().ok_or("source id")?;
        if !alternates || i == 0 {
            episodes.push(
                json!({"id":format!("S01E{:02}",i+1),"base":id,"comparison":"Authored fixture"}),
            );
        }
        let sheet = format!("worksheets/{id}.json");
        let mut rows = workspace::read_record(&work, &sheet)?;
        for row in rows.as_array_mut().ok_or("worksheet array")? {
            row["reviewed"] = json!(true);
            row["action"] = json!("preserve");
        }
        files::write_json(&work.join(sheet), &rows)?;
    }
    if alternates {
        episodes[0]["alternates"] = json!(["s0002"]);
    }
    metadata["episodes"] = json!(episodes);
    files::write_json(&work.join("project.json"), &metadata)?;
    files::write_json(
        &work.join("glossary.json"),
        &json!({"series":"Fixture","terms_reviewed":true,"terms":[],"research":[{"url":"https://example.org/fixture","note":"Authored mechanical test"}]}),
    )?;
    Ok(work)
}
fn build_path(built: &Value) -> Result<PathBuf, String> {
    Ok(PathBuf::from(built["build"].as_str().ok_or("build dir")?))
}

#[test]
fn existing_workspace_and_edition_remain_unchanged() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let work = reviewed(root.path(), &[SOURCE], false)?;
    let stored = work.join("sources/s0001.srt");
    let extra = root.path().join("extra.srt");
    write(&extra, SOURCE.replace("Plain", "New"))?;
    err(prepare(&[extra], &work), "already exists");
    let built = workspace::build(&work, None)?;
    let path = build_path(&built)?.join("Sub/S01E01.ass");
    let before = fs::read(&path).map_err(|e| e.to_string())?;
    assert_eq!(workspace::build(&work, None)?, built);
    assert_eq!(fs::read(path).map_err(|e| e.to_string())?, before);
    assert_eq!(
        fs::read(stored).map_err(|e| e.to_string())?,
        SOURCE.as_bytes()
    );
    Ok(())
}

#[test]
fn no_replace_publisher_does_not_take_existing_empty_directory() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let stage = root.path().join("stage");
    let target = root.path().join("edition");
    for path in [&stage, &target] {
        fs::create_dir(path).map_err(|e| e.to_string())?;
    }
    write(&stage.join("marker"), b"authored bytes")?;
    let error = rename_noreplace(&stage, &target).expect_err("occupied target");
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert!(
        fs::read_dir(&target)
            .map_err(|e| e.to_string())?
            .next()
            .is_none()
    );
    assert_eq!(
        fs::read(stage.join("marker")).map_err(|e| e.to_string())?,
        b"authored bytes"
    );
    Ok(())
}

#[test]
fn publish_bytes_does_not_replace_existing_contents() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let dst = root.path().join("edition.ass");
    publish_bytes(&dst, b"original").map_err(|e| e.to_string())?;
    assert_eq!(
        publish_bytes(&dst, b"replacement").unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(dst).map_err(|e| e.to_string())?, b"original");
    Ok(())
}

#[test]
fn late_episode_failure_leaves_edition_unpublished_and_review_retryable() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let work = reviewed(
        root.path(),
        &[
            SOURCE,
            &SOURCE.replace("Plain dialogue", "<small>Requires repair</small>"),
        ],
        false,
    )?;
    err(workspace::build(&work, None), "markup");
    assert!(!work.join("builds").exists());
    let sheet = "worksheets/s0002.json";
    let mut rows = workspace::read_record(&work, sheet)?;
    rows[0]["text"] = json!("Repaired dialogue");
    rows[0]["action"] = json!("edit");
    rows[0]["structure_note"] = json!("Adapt decoder syntax");
    files::write_json(&work.join(sheet), &rows)?;
    let built = workspace::build(&work, None)?;
    assert_eq!(built["episodes"].as_array().ok_or("episodes")?.len(), 2);
    assert_eq!(workspace::build(&work, None)?, built);
    assert_eq!(workspace::read_record(&work, sheet)?, rows);
    Ok(())
}

#[test]
fn changed_edition_readback_is_refused_instead_of_overwritten() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let work = reviewed(root.path(), &[SOURCE], false)?;
    let built = workspace::build(&work, None)?;
    let dir = build_path(&built)?;
    let subtitle = dir.join("Sub/S01E01.ass");
    let mut body = fs::read(&subtitle).map_err(|e| e.to_string())?;
    let end = body.len() - 2;
    body[end] ^= 1;
    write(&subtitle, &body)?;
    err(workspace::build(&work, None), "modified");
    err(delivery::load_build(&dir), "changed");
    assert_eq!(fs::read(subtitle).map_err(|e| e.to_string())?, body);
    assert_eq!(
        workspace::read_record(&dir, "manifest.json")?["episodes"][0]["sha256"],
        built["episodes"][0]["sha256"]
    );
    Ok(())
}

#[test]
fn donor_provenance_follows_retained_subtitle_and_flooring() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let source = "1\n00:00:01,001 --> 00:00:03,009\nDonor dialogue\n";
    let work = reviewed(root.path(), &[SOURCE, source], true)?;
    let built = workspace::build(&work, None)?;
    let dir = build_path(&built)?;
    let (manifest, docs) = delivery::load_build(&dir)?;
    let record = &manifest["episodes"][0];
    let document = &docs["S01E01"];
    assert_eq!(
        document
            .cues
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>(),
        ["Plain dialogue", "Donor dialogue"]
    );
    let donor = &record["provenance"][1];
    assert_eq!(donor["source"], "s0002");
    assert_eq!(donor["emitted_index"], 2);
    assert_eq!(donor["timing"]["reviewed"], json!([1001, 3009]));
    assert_eq!(donor["timing"]["emitted"], json!([1000, 3000]));
    assert_eq!(donor["timing"]["conversion"], "floor-centisecond");
    assert_eq!(
        record["sha256"],
        digest_bytes(&fs::read(dir.join("Sub/S01E01.ass")).map_err(|e| e.to_string())?)
    );
    Ok(())
}

macro_rules! unsafe_members {
    ($($name:ident: $member:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let root=tempfile::tempdir().map_err(|e|e.to_string())?;
            let archive=root.path().join("input.zip");
            let work=root.path().join("work");
            let mut writer=ZipWriter::new(fs::File::create(&archive).map_err(|e|e.to_string())?);
            writer.start_file($member,SimpleFileOptions::default()).map_err(|e|e.to_string())?;
            writer.write_all(SOURCE.as_bytes()).map_err(|e|e.to_string())?;
            writer.finish().map_err(|e|e.to_string())?;
            err(prepare(&[archive],&work),"Unsafe");
            assert!(!work.exists());
            Ok(())
        }
    )+};
}
unsafe_members! {
    parent_traversal_member_is_refused: "../escape.srt",
    nested_parent_member_is_refused: "nested/../escape.srt",
    absolute_member_is_refused: "/escape.srt",
    drive_qualified_member_is_refused: "C:/escape.srt",
}

fn sampling_doc(n: i64, amend: impl Fn(i64, &mut Cue)) -> Document {
    Document {
        kind: "ass".into(),
        cues: (1..=n)
            .map(|i| {
                let mut cue = Cue {
                    id: format!("c{i:06}"),
                    start: i * 2000 - 1000,
                    end: i * 2000,
                    text: "متن".into(),
                    ..Cue::default()
                };
                cue.fields.insert("style".into(), "Default".into());
                amend(i, &mut cue);
                cue
            })
            .collect(),
        ..Document::default()
    }
}

#[test]
fn legacy_effects_sample_every_event_at_each_phase() -> Check {
    for effect in ["Banner;1;0;0", "Scroll up;0;720;1", "Scroll down;0;720;1"] {
        let plan = sampling::samples(
            &sampling_doc(3, |_, cue| {
                cue.fields.insert("effect".into(), effect.into());
            }),
            false,
            &[],
        )?;
        for index in [1, 2, 3] {
            let times = plan
                .iter()
                .filter(|sample| {
                    sample["cue_indices"]
                        .as_array()
                        .is_some_and(|indices| indices.contains(&json!(index)))
                })
                .map(|sample| &sample["time_seconds"])
                .collect::<Vec<_>>();
            assert_eq!(times.len(), 3, "effect {effect} index {index}");
            if index == 2 {
                assert_eq!(times, [3.1, 3.5, 3.9]);
            }
        }
    }
    Ok(())
}

macro_rules! sample_fields {
    ($($name:ident: ($field:literal,$tag:literal)),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let doc=sampling_doc(3, |i,cue| if $field.is_empty() {
                if i==2 {cue.text=format!("{{{}}}متن",$tag);}
            } else {cue.fields.insert($field.into(),if i==2 {"80".into()} else {"0".into()});});
            let plan=sampling::samples(&doc,false,&[])?;
            assert!(plan.iter().any(|sample|sample["cue_indices"].as_array().is_some_and(|indices|indices.contains(&json!(2)))));
            Ok(())
        }
    )+};
}
sample_fields! {
    layer_creates_distinct_signature: ("layer",""),
    left_margin_creates_distinct_signature: ("marginl",""),
    right_margin_creates_distinct_signature: ("marginr",""),
    vertical_margin_creates_distinct_signature: ("marginv",""),
    rotation_creates_distinct_signature: ("",r"\fr45"),
    blur_creates_distinct_signature: ("",r"\be1"),
    scale_creates_distinct_signature: ("",r"\fsc80"),
}

#[test]
fn static_samples_deduplicate_and_all_cues_obeys_budget() -> Check {
    let doc = sampling_doc(100, |_, _| {});
    assert_eq!(sampling::samples(&doc, false, &[])?.len(), 2);
    assert_eq!(sampling::samples(&doc, true, &[])?.len(), 100);
    let doc = sampling_doc(sampling::MAX_RENDER_FRAMES as i64 + 1, |_, _| {});
    err(sampling::samples(&doc, true, &[]), "budget");
    Ok(())
}

#[test]
fn empty_sample_plan_has_a_controlled_refusal() {
    err(
        sampling::samples(&sampling_doc(0, |_, _| {}), false, &[]),
        "at least one cue",
    );
}

#[test]
fn generation_upgrade_preserves_history_reviews_and_sources() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let work = reviewed(root.path(), &[SOURCE], false)?;
    let edition = workspace::assemble(&work, None)?;
    let input_identity = &edition.manifest["input_identity"];
    let recipe = json!({"version":7,"normalization":7,"timing":"floor-centisecond"});
    let old_id = digest_bytes(
        revayat_core::workflow::contract_json::dump(
            &json!([input_identity, recipe, "ass"]),
            true,
            true,
        )?
        .as_bytes(),
    );
    let old_dir = work.join("builds").join(&old_id);
    fs::create_dir_all(old_dir.join("Sub")).map_err(|e| e.to_string())?;
    fs::create_dir(old_dir.join("renders")).map_err(|e| e.to_string())?;
    let mut old = edition.manifest.clone();
    old["identity"] = json!(old_id);
    old["generation_recipe"] = recipe;
    files::write_json(&old_dir.join("manifest.json"), &old)?;
    files::write_json(&old_dir.join("glossary.json"), &edition.glossary)?;
    for (name, bytes) in &edition.files {
        write(&old_dir.join("Sub").join(name), bytes)?;
    }
    write(
        &old_dir.join("renders/S01E01.json"),
        br#"{"version":2,"note":"Historical marker"}"#,
    )?;
    let paths = [
        work.join("project.json"),
        work.join("sources/s0001.srt"),
        work.join("worksheets/s0001.json"),
        work.join("glossary.json"),
        old_dir.join("manifest.json"),
        old_dir.join("glossary.json"),
        old_dir.join("Sub/S01E01.ass"),
        old_dir.join("renders/S01E01.json"),
    ];
    let snapshot = |paths: &[PathBuf]| {
        paths
            .iter()
            .map(fs::read)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    };
    let before = snapshot(&paths)?;
    let fresh = workspace::build(&work, None)?;
    assert_ne!(fresh["identity"], old["identity"]);
    assert_eq!(fresh["input_identity"], *input_identity);
    assert_eq!(
        fresh["generation_recipe"],
        json!({"version":18,"normalization":15,"timing":"floor-centisecond"})
    );
    assert!(!build_path(&fresh)?.join("renders").exists());
    assert_eq!(snapshot(&paths)?, before);
    Ok(())
}

#[test]
fn reviewed_retiming_keeps_read_order_and_audits_emitted_values() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let base =
        "1\n00:00:00,000 --> 00:00:01,000\nFIRST\n\n2\n00:00:02,000 --> 00:00:03,000\nSECOND\n";
    let work = reviewed(root.path(), &[base], false)?;
    let sheet = "worksheets/s0001.json";
    let mut rows = workspace::read_record(&work, sheet)?;
    rows[0]["start_ms"] = json!(2501);
    rows[0]["end_ms"] = json!(3509);
    rows[0]["timing_note"] = json!("Retime into overlap");
    files::write_json(&work.join(sheet), &rows)?;
    let result = workspace::build(&work, None)?;
    let records = result["episodes"][0]["provenance"]
        .as_array()
        .ok_or("provenance")?;
    assert_eq!(
        [&records[0]["cue"], &records[1]["cue"]],
        ["c000001", "c000002"]
    );
    assert_eq!(
        records[0]["timing"],
        json!({"original":[0,1000],"reviewed":[2501,3509],"emitted":[2500,3500],"conversion":"floor-centisecond"})
    );
    let docs = delivery::load_build(&build_path(&result)?)?.1;
    assert_eq!(
        docs["S01E01"]
            .cues
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>(),
        ["FIRST", "SECOND"]
    );
    Ok(())
}
