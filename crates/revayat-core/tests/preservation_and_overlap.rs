use revayat_core::{
    markup::{self, remap_resets, uncomment},
    runtime::{self, files},
    subtitle_formats::{
        document::{self, Cue, Document},
        has_drawing, structure, visible,
    },
    workflow::{
        import::{self, ImportOptions},
        review, workspace,
    },
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

type Check = Result<(), String>;
const SRT: &str = "1\n00:00:01,000 --> 00:00:02,000\ninput\n";

fn ass(text: &str, headers: &str) -> Result<Document, String> {
    document::parse(
        &format!(
            "[Script Info]\nScriptType: v4.00+\nPlayResX: 320\nPlayResY: 180\n{headers}\n[V4+ Styles]\nFormat: {}\nStyle: {}\n[Events]\nFormat: {}\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,{text}\n",
            document::STYLE_FIELDS,
            document::DEFAULT_STYLE,
            document::ASS_FIELDS
        ),
        "ass",
    )
}

fn err<T: std::fmt::Debug>(result: Result<T, String>, expected: &str) {
    let error = result.expect_err(expected);
    assert!(
        error.contains(expected),
        "{error:?} must contain {expected:?}"
    );
}

fn review_text(
    source: &str,
    text: &str,
    action: &str,
    note: Option<&str>,
) -> Result<Vec<Cue>, String> {
    let doc = ass(source, "")?;
    let row = json!({"id":"c000001","source_text":source,"text":text,"reviewed":true,"action":action,
        "structure_note":note,"start_ms":1000,"end_ms":2000,"links":[]});
    Ok(review::reviewed_cues(&doc, &[row], "fa")?.0)
}

macro_rules! ass_literals {
    ($($name:ident: ($source:literal,$visible:literal,$clean:literal)),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            assert_eq!(visible($source.into(), "ass".into())?, $visible);
            assert_eq!(uncomment($source.into(), "ass".into())?, $clean);
            assert_eq!(uncomment($clean.into(), "ass".into())?, $clean);
            Ok(())
        }
    )+};
}
ass_literals! {
    escaped_braces_are_visible: (r"\{Important\}","{Important}",r"\{Important\}"),
    escaped_opening_brace_is_visible: (r"A\{Important}B","A{Important}B",r"A\{Important}B"),
    double_backslash_before_brace_retains_one: (r"A\\{Important}B",r"A\{Important}B",r"A\\{Important}B"),
    nested_opening_closes_at_first_brace: ("A{comment{inner}VISIBLE}B","AVISIBLE}B","AVISIBLE}B"),
    nested_comment_leaves_prose_visible: ("{comment{inner}VISIBLE}","VISIBLE}","VISIBLE}"),
    later_hidden_comment_is_still_removed: ("A{comment{inner}VISIBLE{hidden}B","AVISIBLEB","AVISIBLEB"),
    unclosed_brace_leaves_tail_literal: ("A{unfinished","A{unfinished","A{unfinished"),
    ordinary_comment_is_hidden: ("A{hidden}B","AB","AB"),
    italic_survives_comment_removal: (r"{\i1}A{note}B{\i0}","AB",r"{\i1}AB{\i0}"),
}

#[test]
fn preserve_action_keeps_escaped_literal_text() -> Check {
    let source = r"A\{Important}B";
    assert_eq!(
        review_text(source, source, "preserve", None)?[0].text,
        source
    );
    Ok(())
}

#[test]
fn nested_comment_cannot_hide_nonempty_cue() {
    let source = "{comment{inner}VISIBLE}";
    err(review_text(source, source, "empty", None), "nonempty");
}

#[test]
fn reset_rewrite_respects_escaped_and_nested_openings() -> Check {
    let mapping = json!({"Default":"Donor"});
    assert_eq!(
        remap_resets(r"\{\rLiteral} then {\rDefault}Hi".into(), mapping.clone())?,
        r"\{\rLiteral} then {\rDonor}Hi"
    );
    let nested = r"A{note{\rDefault}VISIBLE}B";
    assert_eq!(visible(nested.into(), "ass".into())?, "AVISIBLE}B");
    assert_eq!(
        remap_resets(nested.into(), mapping)?,
        r"A{note{\rDonor}VISIBLE}B"
    );
    Ok(())
}

fn check_drawing(source: &str, segments: usize) -> Check {
    assert!(has_drawing(source.into(), "ass".into())?);
    assert_eq!(visible(source.into(), "ass".into())?, "");
    assert_eq!(
        markup::subtitle_pieces(source, "ass")?
            .iter()
            .filter(|(kind, _)| *kind == "drawing")
            .count(),
        segments
    );
    Ok(())
}

macro_rules! drawing_resets {
    ($($name:ident: $tag:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let source=format!(r"{{\p1}}m 0 0 l 20 0 20 20{{{}}}m 30 0 l 50 0 50 20",$tag);
            check_drawing(&source,2)?;
            err(review_text(&source,&source.replace("l 50","l 70"),"edit",None),"tags/drawings");
            Ok(())
        }
    )+};
}
drawing_resets! {
    bare_reset_keeps_drawing: r"\r",
    named_reset_keeps_drawing: r"\rDefault",
    parenthesized_reset_keeps_drawing: r"\r(Default)",
    prefixed_parenthesized_reset_keeps_drawing: r"\rIgnored(Default)",
}

macro_rules! drawing_controls {
    ($($name:ident: $tag:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let source=format!("{{{}}}m 0 0 l 20 0 20 20",$tag);
            check_drawing(&source,1)?;
            err(review_text(&source,&source.replace("l 20","l 70"),"edit",None),"tags/drawings");
            Ok(())
        }
    )+};
}
drawing_controls! {
    bare_transform_drawing_is_structural: r"\t(\p1)",
    timed_transform_drawing_is_structural: r"\t(0,100,\p1)",
    accelerated_transform_drawing_is_structural: r"\t(2,\p(1))",
    timed_accelerated_transform_drawing_is_structural: r"\t(0,100,2,\p1)",
    rejected_transform_does_not_clear_drawing: r"\p1\t(0,1,2,3,\p0)",
    extra_transform_slots_do_not_clear_drawing: r"\p1\t(0,1,2,3,4,\p0)",
    sparse_transform_slots_do_not_clear_drawing: r"\p1\t(0,,1,2,3,\p0)",
    spaced_drawing_control_is_structural: r"\t(\ p1)",
    positive_signed_drawing_control_is_structural: r"\t(\p+1)",
}

#[test]
fn explicit_p0_returns_to_prose_after_resets_and_transforms() -> Check {
    for tag in [r"\p0", r"\t(\p0)", r"\t(0,100,\p(0))"] {
        let source = format!(r"{{\p1}}m 0 0 l 20 0 20 20{{\r}}{{{tag}}}Hello");
        assert_eq!(visible(source.clone(), "ass".into())?, "Hello");
        assert!(has_drawing(source, "ass".into())?);
    }
    for args in ["0,,100,", "0,100,2,0"] {
        assert_eq!(
            visible(format!(r"{{\p1\t({args}\p0)}}Hello"), "ass".into())?,
            "Hello"
        );
    }
    Ok(())
}

#[test]
fn explicit_structure_note_allows_vector_edit() -> Check {
    let source = r"{\p1}m 0 0 l 20 0 20 20{\r}m 30 0 l 50 0 50 20";
    let changed = source.replace("l 50", "l 70");
    assert_eq!(
        review_text(source, &changed, "edit", Some("Authored vector adjustment"))?[0].text,
        changed
    );
    Ok(())
}

#[test]
fn transform_only_vector_is_not_an_empty_cue() {
    let source = r"{\t(\p1)}m 0 0 l 20 0 20 20";
    err(review_text(source, source, "empty", None), "nonempty");
}

#[test]
fn comment_removal_keeps_drawing_object_boundary() -> Check {
    let source = r"{\p1}m 0 0 l 20 0{comment}l 20 20 0 20{\p0}";
    let expected = source.replace("{comment}", "{}");
    assert_eq!(uncomment(source.into(), "ass".into())?, expected);
    assert_eq!(
        review_text(source, source, "preserve", None)?[0].text,
        expected
    );
    assert_eq!(uncomment(expected.clone(), "ass".into())?, expected);
    Ok(())
}

#[test]
fn escaped_override_opening_still_ends_drawing_mode() -> Check {
    let source = r"{\p1}m 0 0 l 20 0 20 20\{\p0}Hello";
    assert_eq!(visible(source.into(), "ass".into())?, "Hello");
    let changed = source.replace("Hello", "سلام");
    assert!(
        visible(
            review_text(source, &changed, "edit", None)?[0].text.clone(),
            "ass".into()
        )?
        .contains("سلام")
    );
    Ok(())
}

#[test]
fn malformed_delimiter_corpus_is_literal() -> Check {
    for (kind, body) in [
        ("srt", format!("{}tail", "<!--".repeat(50_000))),
        ("ass", format!("{}tail", "{".repeat(50_000))),
        ("srt", format!("<b{}", " ".repeat(50_000))),
    ] {
        assert_eq!(uncomment(body.clone(), kind.into())?, body);
    }
    Ok(())
}

#[test]
fn srt_comments_and_break_dialect_preserve_literals() -> Check {
    assert_eq!(uncomment("A<!--x\ny-->B".into(), "srt".into())?, "AB");
    assert_eq!(
        visible("A<!--x-->B<br a=\"b\">C".into(), "srt".into())?,
        "AB\nC"
    );
    let literal = "A<br\t>B<br\u{a0}>C &amp; x < 5 > y";
    assert_eq!(visible(literal.into(), "srt".into())?, literal);
    assert_eq!(
        uncomment("A<!--open<b>B</b>".into(), "srt".into())?,
        "A<!--open<b>B</b>"
    );
    Ok(())
}

#[test]
fn comment_removal_preserves_seeded_renderer_text() -> Check {
    let mut seed = 1003_u64;
    for _ in 0..250 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let (kind, comment) = if seed & 0x100 == 0 {
            ("ass", "{comment}")
        } else {
            ("srt", "<!--comment-->")
        };
        let pieces = ["A", "سلام", "〈x〉", "&amp;", " "];
        let plain = format!(
            "{}{}",
            pieces[(seed >> 22) as usize % pieces.len()],
            pieces[(seed >> 37) as usize % pieces.len()]
        );
        let split = plain.char_indices().nth(1).map_or(plain.len(), |(i, _)| i);
        let source = format!("{}{comment}{}", &plain[..split], &plain[split..]);
        let cleaned = uncomment(source.clone(), kind.into())?;
        assert_eq!(cleaned, plain);
        assert_eq!(
            visible(cleaned, kind.into())?,
            visible(source, kind.into())?
        );
    }
    Ok(())
}

macro_rules! globals {
    ($($name:ident: $field:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let mut base=ass("Base",&format!("{}: one",$field))?;
            let donor=ass("Donor",&format!("{}: two",$field))?;
            let before=(base.clone(),donor.clone());
            let error=review::donor(&mut base,&donor,donor.cues.clone(),"s0002",true).expect_err("mismatched globals");
            assert!(error.contains("settings") && error.contains(&$field.to_ascii_lowercase()),"{error}");
            assert_eq!((base,donor),before);
            Ok(())
        }
    )+};
}
globals! {
    kerning_mismatch_is_atomic: "Kerning",
    layout_x_mismatch_is_atomic: "LayoutResX",
    layout_y_mismatch_is_atomic: "LayoutResY",
    language_mismatch_is_atomic: "Language",
    color_matrix_mismatch_is_atomic: "YCbCr Matrix",
    collisions_mismatch_is_atomic: "Collisions",
    timer_mismatch_is_atomic: "Timer",
}

#[test]
fn noncanonical_global_shadows_refuse_before_mutation() -> Check {
    for (base_header, donor_header) in [
        ("Kerning: yes\nkerning: no", "Kerning: no"),
        ("Kerning: yes\nKerning : no", "Kerning: no"),
        ("Kerning: yes\nKERNING: no", "Kerning: no"),
        ("Kerning: yes", "kerning: yes"),
    ] {
        let mut base = ass("Base", base_header)?;
        let donor = ass("Donor", donor_header)?;
        let before = (base.clone(), donor.clone());
        err(
            review::donor(&mut base, &donor, donor.cues.clone(), "s0002", true),
            "settings",
        );
        assert_eq!((base, donor), before);
    }
    Ok(())
}

#[test]
fn matching_globals_and_empty_donor_are_accepted() -> Check {
    let headers = "Kerning: yes\nLayoutResX: 320\nLayoutResY: 180\nLanguage: fa";
    let mut base = ass("Base", headers)?;
    let donor = ass("Donor", headers)?;
    let merged = review::donor(&mut base, &donor, donor.cues.clone(), "s0002", true)?;
    assert_eq!(merged[0].fields["style"], "s0002__Default");
    let incompatible = ass("Donor", "Kerning: no")?;
    let before = base.clone();
    assert!(review::donor(&mut base, &incompatible, vec![], "s0003", true)?.is_empty());
    assert_eq!(base, before);
    Ok(())
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) -> Check {
    fs::write(path, bytes).map_err(|e| e.to_string())
}
fn prepare(inputs: &[PathBuf], work: &Path, encoding: &str) -> Result<Value, String> {
    import::prepare(
        inputs,
        work,
        &ImportOptions {
            series: "Authored audit",
            season: 1,
            encoding,
            glossary: None,
            language: "en",
            output_format: "ass",
        },
    )
}

#[test]
fn later_parent_excludes_its_own_generated_stage() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let source = root.path().join("input.srt");
    let work = root.path().join("work");
    write(&source, SRT)?;
    let project = prepare(&[source.clone(), root.path().into()], &work, "utf-8")?;
    let sources = project["sources"].as_array().ok_or("sources")?;
    assert_eq!(sources.len(), 2);
    assert!(
        sources
            .iter()
            .all(|s| s["origin"]["relative_path"] == "input.srt")
    );
    assert_eq!(
        fs::read(&source).map_err(|e| e.to_string())?,
        SRT.as_bytes()
    );
    assert_eq!(workspace::load(&work)?.documents.len(), 2);
    Ok(())
}

#[test]
fn repeated_directory_keeps_declared_root_ids() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    write(&root.path().join("input.srt"), SRT)?;
    let project = prepare(
        &vec![root.path().to_owned(); 3],
        &root.path().join("work"),
        "utf-8",
    )?;
    let ids: Vec<_> = project["sources"]
        .as_array()
        .ok_or("sources")?
        .iter()
        .map(|s| &s["origin"]["root_id"])
        .collect();
    assert_eq!(ids, ["input001", "input002", "input003"]);
    Ok(())
}

#[test]
fn zip_then_parent_keeps_member_provenance() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let archive = root.path().join("input.zip");
    let mut writer = zip::ZipWriter::new(fs::File::create(&archive).map_err(|e| e.to_string())?);
    writer
        .start_file("nested/input.srt", zip::write::SimpleFileOptions::default())
        .map_err(|e| e.to_string())?;
    writer
        .write_all(SRT.as_bytes())
        .map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    let project = prepare(
        &[archive, root.path().into()],
        &root.path().join("work"),
        "utf-8",
    )?;
    let sources = project["sources"].as_array().ok_or("sources")?;
    assert_eq!(sources.len(), 2);
    for s in sources {
        assert_eq!(s["origin"]["member"], "nested/input.srt");
        assert!(
            !s["name"]
                .as_str()
                .ok_or("name")?
                .contains(".subtitle-import-")
        );
    }
    Ok(())
}

#[test]
fn authored_stage_lookalike_is_not_excluded_by_name() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let authored = root.path().join(".subtitle-import-authored");
    fs::create_dir(&authored).map_err(|e| e.to_string())?;
    let source = authored.join("keep.srt");
    write(&source, SRT)?;
    let project = prepare(
        &[source, root.path().into()],
        &root.path().join("work"),
        "utf-8",
    )?;
    assert_eq!(project["sources"].as_array().ok_or("sources")?.len(), 2);
    assert!(
        project["sources"][1]["name"]
            .as_str()
            .ok_or("name")?
            .contains(".subtitle-import-authored/keep.srt")
    );
    Ok(())
}

#[test]
fn owned_stage_exclusion_is_a_path_boundary_and_handles_parent_segments() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (owned, neighbor) = (
        root.path().join("stage"),
        root.path().join("stage-original"),
    );
    for dir in [&owned, &neighbor] {
        fs::create_dir(dir).map_err(|e| e.to_string())?;
        write(&dir.join("keep.srt"), SRT)?;
    }
    assert_eq!(
        import::input_candidates(&neighbor.join(".."), Some(&owned))?
            .iter()
            .map(|p| p.canonicalize())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?,
        [neighbor
            .join("keep.srt")
            .canonicalize()
            .map_err(|e| e.to_string())?]
    );
    assert!(import::input_candidates(&owned, Some(&owned))?.is_empty());
    Ok(())
}

#[test]
fn late_source_failure_preserves_originals_and_retry_inventory() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (good, bad, work) = (
        root.path().join("good.srt"),
        root.path().join("invalid.srt"),
        root.path().join("work"),
    );
    write(&good, SRT)?;
    write(&bad, "not a subtitle")?;
    assert!(prepare(&[good.clone(), root.path().into()], &work, "utf-8").is_err());
    assert!(!work.exists());
    assert_eq!(fs::read(&good).map_err(|e| e.to_string())?, SRT.as_bytes());
    write(&bad, SRT)?;
    let project = prepare(&[good, root.path().into()], &work, "utf-8")?;
    assert_eq!(project["sources"].as_array().ok_or("sources")?.len(), 3);
    for entry in fs::read_dir(root.path()).map_err(|e| e.to_string())? {
        assert!(
            !entry
                .map_err(|e| e.to_string())?
                .file_name()
                .to_string_lossy()
                .starts_with(".subtitle-import-")
        );
    }
    Ok(())
}

#[test]
fn bounded_staging_lifetime_cleans_owned_directory() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let stage = workspace::stage(root.path(), ".owned-")?;
    let path = stage.path().to_owned();
    write(&path.join("data"), "Authored payload")?;
    drop(stage);
    assert!(!path.exists());
    Ok(())
}

macro_rules! breaks {
    ($($name:ident: ($tag:literal,$expected:literal)),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let text=format!("A{}B",$tag);
            assert_eq!(visible(text.clone(),"srt".into())?,$expected);
            let cue=document::srt_to_ass(Cue { text,start:1000,end:2000,..Cue::default() })?;
            assert_eq!(visible(cue.text,"ass".into())?,$expected);
            Ok(())
        }
    )+};
}
breaks! {
    plain_br: ("<br>","A\nB"),
    compact_self_closing_br: ("<br/>","A\nB"),
    spaced_self_closing_br: ("<br />","A\nB"),
    uppercase_br: ("<BR>","A\nB"),
    mixed_case_multi_space_br: ("<Br  />","A\nB"),
    trailing_space_br: ("<br/ >","A\nB"),
    attribute_br: ("<br class='line'>","A\nB"),
    tab_br_is_literal: ("<br\t>","A<br\t>B"),
    nbsp_br_is_literal: ("<br\u{a0}>","A<br\u{a0}>B"),
}

#[test]
fn break_only_cue_is_empty_and_duplicate_breaks_are_removed() -> Check {
    assert_eq!(visible("<br><br/>".into(), "srt".into())?, "");
    assert_eq!(
        markup::clean_empty_lines("A<br><br>B".into(), "srt".into(), false)?,
        "A\nB"
    );
    let text = "<i>Hi</i> x < 5 > y &amp; <break>";
    assert_eq!(
        visible(text.into(), "srt".into())?,
        "Hi x < 5 > y &amp; <break>"
    );
    Ok(())
}

#[test]
fn srt_break_variants_anchor_each_persian_display_line() -> Check {
    for tag in ["<br>", "<br/>", "<br />", "<BR>", "<Br  />"] {
        let text = format!("سلام{tag}دنیا OVA");
        let normalized = markup::rtl(text.clone(), "srt".into(), "rtl".into(), false)?;
        assert_eq!(visible(text, "srt".into())?, "سلام\nدنیا OVA");
        assert_eq!(normalized.matches('\u{200f}').count(), 4);
        assert_eq!(visible(normalized.clone(), "srt".into())?, "سلام\nدنیا OVA");
        assert_eq!(
            markup::rtl(normalized.clone(), "srt".into(), "rtl".into(), false)?,
            normalized
        );
    }
    Ok(())
}

macro_rules! scalar {
    ($($name:ident: ($text:literal,$tag:literal,$arg:literal)),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            assert_eq!(document::ass_commands($text)?,[($tag.into(),$arg.into())]);
            Ok(())
        }
    )+};
}
scalar! {
    reset_parentheses_precede_prefix: (r"{\rIgnored(Default)}","r","Default"),
    reset_parentheses_precede_space: (r"{\r (Default)}","r","Default"),
    drawing_parentheses_precede_digit: (r"{\p1(0)}","p","0"),
    empty_drawing_parentheses_use_prefix: (r"{\p1(,,)}","p","1"),
    empty_reset_parentheses_use_prefix: (r"{\rDefault()}","r","Default"),
    font_parentheses_precede_prefix: (r"{\fnIgnored(DejaVu Sans)}","fn","DejaVu Sans"),
}

#[test]
fn reset_references_and_parenthesized_scalars_are_preserved() -> Check {
    let cue = Cue {
        text: r"{\r(Default)}Hi".into(),
        ..Cue::default()
    };
    assert_eq!(document::style_references(&cue)?, ["Default"]);
    let mapping = json!({"Default":"Donor"});
    assert_eq!(
        remap_resets(r"{\r(Default)}literal \rDefault".into(), mapping.clone())?,
        r"{\r(Donor)}literal \rDefault"
    );
    assert_eq!(
        remap_resets(r"{\rIgnored(Default)}x".into(), mapping.clone())?,
        r"{\rIgnored(Donor)}x"
    );
    assert_eq!(
        remap_resets(r"{\t(0,100,\rDefault)\r}x".into(), mapping)?,
        r"{\t(0,100,\rDonor)\r}x"
    );
    assert!(document::ass_commands(r"{\t(0,100,\fs42}").is_err());
    assert_eq!(
        document::ass_commands(r"{\fn(DejaVu Sans)\p(1)\p(0)\fs(32)}")?,
        [
            ("fn".into(), "DejaVu Sans".into()),
            ("p".into(), "1".into()),
            ("p".into(), "0".into()),
            ("fs".into(), "32".into())
        ]
    );
    assert_eq!(
        document::ass_commands(r"{\fr45\be1\fsc100}")?,
        [
            ("fr".into(), "45".into()),
            ("be".into(), "1".into()),
            ("fsc".into(), "100".into())
        ]
    );
    assert!(has_drawing(
        r"{\p(1)}m 0 0 l 1 1{\p(0)}Hi".into(),
        "ass".into()
    )?);
    let doc = ass(r"{\rIgnored(Default)\p1(0)}Hi", "")?;
    assert!(document::serialize(&doc, &doc.cues, false)?.contains("Hi"));
    Ok(())
}

#[test]
fn cross_line_isolates_and_embeddings_preserve_scope_and_prose() -> Check {
    for (kind, newline) in [("ass", r"\N"), ("ass", r"\n"), ("srt", "\n")] {
        for (opening, closing) in [
            ('\u{2066}', '\u{2069}'),
            ('\u{2067}', '\u{2069}'),
            ('\u{2068}', '\u{2069}'),
            ('\u{202a}', '\u{202c}'),
            ('\u{202b}', '\u{202c}'),
        ] {
            for (a, b) in [("سلام OVA", "دنیا"), ("سلام", "دنیا OVA")] {
                let source = format!("{opening}{a}{newline}{b}{closing}");
                let rtl = |text| markup::rtl(text, kind.into(), "rtl".into(), false);
                let result = rtl(source.clone())?;
                markup::validate_bidi(result.clone(), kind.into())?;
                assert_eq!(
                    visible(result.clone(), kind.into())?,
                    visible(source, kind.into())?
                );
                assert_eq!(rtl(result.clone())?, result);
                assert_eq!(result.matches(opening).count(), 1);
                assert_eq!(result.matches(closing).count(), 1);
            }
        }
    }
    Ok(())
}

#[test]
fn nested_cross_line_isolates_keep_italic_and_validate() -> Check {
    let source = "\u{2067}سلام {\\i1}OVA\\N\u{202a}English\\Nدنیا\u{2069}";
    let result = markup::rtl(source.into(), "ass".into(), "rtl".into(), false)?;
    markup::validate_bidi(result.clone(), "ass".into())?;
    assert_eq!(
        markup::rtl(result.clone(), "ass".into(), "rtl".into(), false)?,
        result
    );
    assert_eq!(
        visible(result.clone(), "ass".into())?,
        visible(source.into(), "ass".into())?
    );
    assert!(result.contains(r"{\i1}"));
    Ok(())
}

#[test]
fn invalid_user_scopes_are_not_repaired() {
    for text in [
        "\u{2067}سلام",
        "\u{202c}سلام",
        "\u{2069}",
        "\u{2067}سلام\u{202c}\u{2069}",
    ] {
        assert!(
            markup::rtl(text.into(), "srt".into(), "rtl".into(), false).is_err(),
            "{text:?}"
        );
    }
}

macro_rules! text_only {
    ($($name:ident: $encoding:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let root=tempfile::tempdir().map_err(|e|e.to_string())?;
            let file=root.path().join("input.srt");
            let work=root.path().join("work");
            write(&file,SRT)?;
            err(prepare(&[file],&work,$encoding),"encoding");
            assert!(!work.exists());
            Ok(())
        }
    )+};
}
text_only! {
    base64_is_not_a_subtitle_codec: "base64",
    rot13_is_not_a_subtitle_codec: "rot13",
    hex_is_not_a_subtitle_codec: "hex",
    bz2_is_not_a_subtitle_codec: "bz2",
    zlib_is_not_a_subtitle_codec: "zlib",
}

#[test]
fn text_encodings_import_without_losing_persian() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let source = SRT.replace("input", "سلام");
    let utf16 = source
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let mut cp1256 = SRT.as_bytes().to_vec();
    cp1256.splice(cp1256.len() - 6..cp1256.len() - 1, [0xd3, 0xe1, 0xc7, 0xe3]);
    for (encoding, raw) in [
        ("utf-8", source.into_bytes()),
        ("utf-16", [&[0xff, 0xfe][..], &utf16].concat()),
        ("utf-16-le", utf16),
        ("cp1256", cp1256),
    ] {
        let input = root.path().join(format!("{encoding}.srt"));
        let work = root.path().join(encoding);
        write(&input, &raw)?;
        prepare(&[input], &work, encoding)?;
        assert_eq!(
            workspace::load(&work)?.documents["s0001"].cues[0].text,
            "سلام"
        );
        assert_eq!(
            fs::read(work.join("sources/s0001.srt")).map_err(|e| e.to_string())?,
            raw
        );
    }
    Ok(())
}

#[test]
fn bounded_read_checks_numeric_limit_before_touching_missing_path() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = root.path().join("missing.json");
    err(
        runtime::read_limited(revayat_core::workflow::contract::path_marker(&path), -1),
        "limit",
    );
    write(&path, b"123")?;
    err(files::read_limited(&path, 2), "limit");
    assert_eq!(files::read_limited(&path, 3)?, b"123");
    Ok(())
}

#[test]
fn many_ass_sections_keep_order_and_reject_casefolded_duplicate() -> Check {
    let doc = ass("Visible", "")?;
    let mut source = document::serialize(&doc, &doc.cues, false)?;
    for i in 0..6000 {
        use std::fmt::Write;
        writeln!(source, "[Custom {i}]\nkey: value").map_err(|e| e.to_string())?;
    }
    let parsed = document::parse(&source, "ass")?;
    assert_eq!(parsed.sections.len(), 6003);
    assert_eq!(
        parsed.sections.last().ok_or("last section")?.0,
        "[Custom 5999]"
    );
    assert!(document::serialize(&parsed, &parsed.cues, false)?.contains("[Custom 5999]"));
    err(
        document::parse(&(source + "\n[CUSTOM 0]\n"), "ass"),
        "Repeated",
    );
    Ok(())
}

#[test]
fn preserved_drawing_structure_changes_when_coordinates_change() -> Check {
    let source = r"{\p1}m 0 0 l 20 0 20 20{\p0}Text";
    assert_ne!(
        structure(source.into(), "ass".into())?,
        structure(source.replace("20", "70"), "ass".into())?
    );
    Ok(())
}
