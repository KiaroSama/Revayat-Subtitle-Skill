use revayat_core::{
    markup,
    runtime::files::write_json,
    subtitle_formats::{
        conversion::{self, Options},
        document::{self, Cue, Document},
        visible,
    },
    text_pipeline as text,
    workflow::{import, workspace},
};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};

type Check = Result<(), String>;
const RLM: char = '\u{200f}';
const RLI: char = '\u{2067}';
const PDI: char = '\u{2069}';

fn options() -> Options<'static> {
    Options {
        direction: "ltr",
        fix_zwnj: false,
        wrap_lines: false,
        ..Options::default()
    }
}

fn ass(body: &str) -> Result<Document, String> {
    let source = format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: 320\nPlayResY: 180\n[V4+ Styles]\nFormat: {}\nStyle: {}\n[Events]\nFormat: {}\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,{body}\n",
        document::STYLE_FIELDS,
        document::DEFAULT_STYLE,
        document::ASS_FIELDS
    );
    document::parse(&source, "ass")
}

fn srt(body: &str) -> Document {
    Document {
        kind: "srt".into(),
        cues: vec![Cue {
            id: "c000001".into(),
            start: 1000,
            end: 3000,
            text: body.into(),
            ..Cue::default()
        }],
        ..Document::default()
    }
}

macro_rules! ass_literal {
    ($($name:ident: $body:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let original = ass($body)?;
            let style = original.styles["Default"].join(",");
            let (converted, removed) = conversion::ass_document_to_ass(&original, &Options { strip_watermarks:false, style:Some(&style), ..options() })?;
            assert!(removed.is_empty());
            assert_eq!(converted.cues[0].text, $body);
            assert_eq!(original.cues[0].text, $body);
            let rendered = document::serialize(&converted, &converted.cues, false)?;
            assert_eq!(document::parse(&rendered, "ass")?.cues[0].text, $body);
            Ok(())
        }
    )+};
}

ass_literal! {
    f01_literal_italic: "Type <i> here.",
    f01_literal_break: "Use <br> literally.",
    f01_literal_font_color: r##"<font color="#FF0000">red</font> is the displayed example."##,
    f01_italic_override_with_literal_bold: r"{\i1}Read <b> literally.{\i0}",
    f01_literal_html_inside_font_override: r"{\fnKeep <i> literal}Display <i> in the subtitle.",
    f01_literal_tags_through_regular_conversion: "Print <i> and <br> literally.",
}

#[test]
fn f01_wrap_counts_literal_ass_html_as_visible_prose() -> Check {
    let input = "<font color=red>one two three</font> four five";
    let wrapped = text::wrap_subtitle_lines(input.into(), Some(24), Some(2))?;
    assert_ne!(wrapped, input);
    assert_eq!(wrapped.matches(r"\N").count(), 1);
    let shown = visible(wrapped, "ass".into())?;
    assert_eq!(shown.replace('\n', " "), input);
    assert!(shown.split('\n').next().ok_or("line")?.chars().count() <= 24);
    Ok(())
}

#[test]
fn f01_rtl_anchor_sees_letters_inside_literal_ass_html() -> Check {
    let text = "<b title=\"سلام\">.</b>";
    let anchor = |body| text::anchor_rtl_line(body, Some("ass".into()), Some("rtl".into()));
    let anchored = anchor(text.into())?;
    assert!(anchored.starts_with('\u{202b}'));
    assert_eq!(visible(anchored.clone(), "ass".into())?, text);
    assert_eq!(anchor(anchored.clone())?, anchored);
    markup::validate_bidi(anchored, "ass".into())
}

#[test]
fn f05_unclosed_comments_do_not_absorb_following_persian_prose() -> Check {
    let malformed = "<!--".repeat(100_000);
    assert_eq!(text::watermark_reason(malformed.clone())?, "");
    assert_eq!(text::normalize_persian_zwnj(malformed.clone())?, malformed);
    assert_eq!(
        text::convert_srt_markup_to_ass(malformed.clone())?,
        malformed
    );
    assert_eq!(
        text::normalize_persian_zwnj(format!("{malformed}مي روم كتاب ها خانه ام هم چنین"))?,
        format!("{malformed}می‌روم کتاب‌ها خانه‌ام همچنین")
    );
    Ok(())
}

#[test]
fn f05_completed_comments_and_ass_override_text_are_protected() -> Check {
    assert_eq!(
        text::normalize_persian_zwnj("می روم <!-- مي روم <i>hidden</i> --> می روم".into())?,
        "می‌روم <!-- مي روم <i>hidden</i> --> می‌روم"
    );
    let hidden = "A<!--<i>hidden</i>-->B";
    assert_eq!(text::convert_srt_markup_to_ass(hidden.into())?, hidden);
    assert_eq!(
        text::watermark_reason("Subtitles by Alice<!--not dialogue-->".into())?,
        "fansub-credit"
    );
    let control = r"{\fnمي <!--x--> <i> literal}word";
    assert_eq!(text::normalize_persian_zwnj(control.into())?, control);
    assert_eq!(text::convert_srt_markup_to_ass(control.into())?, control);
    assert_eq!(
        text::convert_srt_markup_to_ass("A<!--{-->B<i>C</i>".into())?,
        r"A<!--{-->B{\i1}C{\i0}"
    );
    Ok(())
}

macro_rules! no_watermark {
    ($($name:ident: $body:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            assert_eq!(text::watermark_reason($body.into())?, "");
            let original = srt(&$body.replace(r"\N", "\n"));
            let (converted, removed) = conversion::convert(&original, &options())?;
            assert!(removed.is_empty());
            assert_eq!(converted.cues.len(), 1);
            assert_eq!(visible(converted.cues[0].text.clone(), "ass".into())?, visible(original.cues[0].text.clone(), "srt".into())?);
            Ok(())
        }
    )+};
}

no_watermark! {
    f02_downloaded_device_dialogue: "Downloaded from the device, the files contain evidence.",
    f02_amateur_subtitles_dialogue: "Subtitles by amateurs can contain mistakes.",
    f02_amateur_subtitles_predicate: "Subtitles by amateurs are unreliable.",
    f02_uppercase_quoted_predicate: "«SUBTITLES BY AMATEURS ARE UNRELIABLE!»",
    f02_human_subtitles_predicate: "Subtitles by humans fail.",
    f02_human_captions_predicate: "Captioned by humans looks better.",
    f02_machine_encoding_predicate: "Encoded by machines looks better.",
    f02_credit_prefix_with_physical_break: "Subtitles by amateurs\nare unreliable.",
    f02_credit_prefix_with_decoder_break: "Subtitles by amateurs<br>are unreliable.",
    f02_ambiguous_full_name: "Subtitles by John O'Connor",
    f02_downloaded_from_web_prose: "Downloaded from the web",
    f02_persian_credit_like_prose: "زیرنویس از دو دوست",
    f02_spoken_url: "Open https://example.test to read the manual.",
    f02_spoken_provider: "They used OpenSubtitles for the experiment.",
    f02_spoken_telegram_handle: "Send that address, t.me/channel, to your friend.",
    f02_spoken_studio: "Deluxe Media is mentioned in the documentary.",
    f02_credit_inside_question: "I asked who the subtitles by Alice were for.",
    f02_spoken_persian_telegram: "من این کانال تلگرام را نمی‌شناسم.",
    f02_dialogue_then_url_physical: "Save the evidence.\nhttps://example.test",
    f02_dialogue_then_url_html: "Save the evidence.<br>https://example.test",
    f02_dialogue_then_handle_hard_break: "Save the evidence.\\N@channel",
}

macro_rules! verdict {
    ($($name:ident: ($kind:literal, $body:literal, $reason:literal)),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            assert_eq!(text::watermark_reason_with_kind($body.into(), $kind)?, $reason);
            assert_eq!(text::is_watermark_cue_with_kind($body.into(), $kind)?, !$reason.is_empty());
            Ok(())
        }
    )+};
}

verdict! {
    f02_standalone_url: ("srt", "https://example.test", "url-ad"),
    f02_bracketed_url: ("srt", "[www.example.test]", "url-ad"),
    f02_standalone_handle: ("srt", "@channel", "telegram-handle"),
    f02_telegram_url: ("srt", "https://t.me/channel", "telegram-handle"),
    f02_site_name: ("srt", "OpenSubtitles", "site-watermark"),
    f02_site_domain: ("srt", "OpenSubtitles.org", "site-watermark"),
    f02_single_name_credit: ("srt", "Subtitles by Alice", "fansub-credit"),
    f02_apostrophe_name_credit: ("srt", "Subtitles by O'Connor", "fansub-credit"),
    f02_hyphenated_name_credit: ("srt", "Subtitles by Alice-Smith", "fansub-credit"),
    f02_team_credit: ("srt", "Subtitles by @actual_team", "fansub-credit"),
    f02_url_credit: ("srt", "Subtitles by https://example.test", "fansub-credit"),
    f02_download_url_credit: ("srt", "Downloaded from https://example.test", "fansub-credit"),
    f02_french_credit: ("srt", "Sous-titres : Alice", "studio-credit"),
    f02_formatted_credit_and_url: ("srt", "<i>Subtitles by Alice</i><br>https://example.test", "fansub-credit"),
    f02_srt_unknown_notice: ("srt", "<notice>https://example.test</notice>", ""),
    f02_srt_literal_attribute: ("srt", "<i class=\"literal\">https://example.test</i>", ""),
    f02_srt_pos_is_literal: ("srt", r"{\pos(1,2)}https://example.test", ""),
    f02_srt_braces_are_literal: ("srt", "{literal}https://example.test", ""),
    f02_srt_real_italic: ("srt", "<i>https://example.test</i>", "url-ad"),
    f02_srt_legacy_alignment: ("srt", "{an8}https://example.test", "url-ad"),
    f02_srt_backslash_alignment: ("srt", r"{\an8}https://example.test", "url-ad"),
    f02_srt_hidden_comment: ("srt", "<!--hidden-->https://example.test", "url-ad"),
    f02_ass_html_is_literal: ("ass", "<i>https://example.test</i>", ""),
    f02_ass_html_comment_is_literal: ("ass", "<!--literal-->https://example.test", ""),
    f02_ass_escaped_braces_are_literal: ("ass", r"\{literal\}https://example.test", ""),
    f02_ass_real_italic: ("ass", r"{\i1}https://example.test{\i0}", "url-ad"),
    f02_ass_hidden_comment: ("ass", "{hidden}https://example.test", "url-ad"),
    f02_ass_drawing_remains: ("ass", r"{\p1}m 0 0 l 1 1{\p0}https://example.test", ""),
}

#[test]
fn f02_literal_suffixes_never_become_automatic_ads() -> Check {
    for (link, reason) in [
        ("https://example.test", "url-ad"),
        ("OpenSubtitles.org/path", "site-watermark"),
        ("t.me/channel", "telegram-handle"),
    ] {
        for kind in ["ass", "srt"] {
            assert_eq!(text::watermark_reason_with_kind(link.into(), kind)?, reason);
            for suffix in [
                "<notice>",
                "</notice>",
                if kind == "srt" {
                    "{literal}"
                } else {
                    r"\{literal\}"
                },
            ] {
                assert_eq!(
                    text::watermark_reason_with_kind(format!("{link}{suffix}"), kind)?,
                    ""
                );
            }
        }
        assert_eq!(
            text::watermark_reason_with_kind(format!("<notice>{link}</notice>"), "ass")?,
            ""
        );
    }
    Ok(())
}

#[test]
fn f02_unknown_format_is_rejected_consistently() {
    for error in [
        text::watermark_reason_with_kind("https://example.test".into(), "vtt").unwrap_err(),
        text::is_watermark_cue_with_kind("https://example.test".into(), "vtt").unwrap_err(),
    ] {
        assert!(
            error.contains("Watermark format must be ass or srt"),
            "{error}"
        );
    }
}

macro_rules! reject_decoder {
    ($($name:ident: $body:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let raw = format!("1\n00:00:01,000 --> 00:00:02,000\n{}\n\n2\n00:00:03,000 --> 00:00:04,000\nRetained dialogue\n", $body);
            let doc = document::parse(&raw, "srt")?;
            let before = doc.clone();
            let error = conversion::convert(&doc, &options()).err().ok_or("unsupported syntax was accepted")?;
            assert!(error.contains("unsupported decoder markup") || error.contains("ASS control syntax"), "{error}");
            assert_eq!(doc, before);
            Ok(())
        }
    )+};
}

reject_decoder! {
    f02_reject_unknown_tag_before_filtering: "<notice>https://example.test</notice>",
    f02_reject_attributed_italic_before_filtering: "<i class=\"literal\">https://example.test</i>",
    f02_reject_ass_position_before_filtering: r"{\pos(1,2)}https://example.test",
    f02_reject_braces_before_filtering: "{literal}https://example.test",
    f02_reject_closing_tag_after_ad: "https://example.test</notice>",
    f02_reject_opening_tag_after_site: "OpenSubtitles.org/path<notice>",
    f02_reject_literal_braces_after_handle: "t.me/channel{literal}",
}

#[test]
fn f02_ass_literal_styles_and_escaped_braces_survive_real_ad_filter() -> Check {
    for (link, reason) in [
        ("https://example.test", "url-ad"),
        ("OpenSubtitles.org/path", "site-watermark"),
        ("t.me/channel", "telegram-handle"),
    ] {
        let mut original = ass(&format!("<i>{link}</i>"))?;
        for (i, body) in [format!(r"\{{literal\}}{link}"), link.into()]
            .into_iter()
            .enumerate()
        {
            let mut cue = original.cues[0].clone();
            cue.id = format!("c{:06}", i + 2);
            cue.start += ((i + 1) * 3000) as i64;
            cue.end += ((i + 1) * 3000) as i64;
            cue.text = body;
            original.cues.push(cue);
        }
        let (doc, report) = conversion::convert(&original, &options())?;
        assert_eq!(
            doc.cues.iter().map(|c| &c.text).collect::<Vec<_>>(),
            [&original.cues[0].text, &original.cues[1].text]
        );
        assert_eq!(report.len(), 1);
        assert_eq!(report[0]["id"], "c000003");
        assert_eq!(report[0]["reason"], reason);
        assert_eq!(original.cues[2].text, link);
    }
    Ok(())
}

#[test]
fn f02_long_srt_tag_stays_literal_and_is_not_an_ad() -> Check {
    let body = format!("<i title=\"{}\">https://example.test</i>", "x".repeat(128));
    assert_eq!(text::watermark_reason_with_kind(body.clone(), "srt")?, "");
    let (doc, removed) = conversion::convert(&srt(&body), &options())?;
    assert!(removed.is_empty());
    assert_eq!(
        visible(doc.cues[0].text.clone(), "ass".into())?,
        visible(body, "srt".into())?
    );
    Ok(())
}

#[test]
fn f02_translator_donor_literal_html_keeps_placeholder() -> Check {
    let mut donor = ass("<i>https://example.test</i>")?;
    donor.cues[0].start = 4000;
    donor.cues[0].end = 5000;
    let (_, report) = conversion::translator_ass(
        &srt("Retained dialogue"),
        &[("donor".into(), donor)],
        false,
        &options(),
    )?;
    assert_eq!(report["placeholder_slots"], 1);
    assert_eq!(report["watermarks_removed"], 0);
    Ok(())
}

fn reviewed(root: &Path, bodies: &[&str]) -> Result<PathBuf, String> {
    let mut inputs = Vec::new();
    for (index, body) in bodies.iter().enumerate() {
        let path = root.join(format!("source-{index}.srt"));
        fs::write(&path, body).map_err(|e| e.to_string())?;
        inputs.push(path);
    }
    let work = root.join("work");
    import::prepare(
        &inputs,
        &work,
        &import::ImportOptions {
            series: "Audit fixture",
            season: 1,
            encoding: "utf-8",
            glossary: None,
            language: "en",
            output_format: "ass",
        },
    )?;
    let mut project = workspace::read_record(&work, "project.json")?;
    let ids: Vec<_> = project["sources"]
        .as_array()
        .ok_or("sources")?
        .iter()
        .map(|source| source["id"].clone())
        .collect();
    project["episodes"] = json!([{"id":"S01E01","base":"s0001","alternates":&ids[1..],"comparison":"Authored accounting fixture"}]);
    write_json(&work.join("project.json"), &project)?;
    write_json(
        &work.join("glossary.json"),
        &json!({"series":"Audit fixture","terms_reviewed":true,"terms":[],"research":[{"url":"https://example.org/fixture","note":"Authored fixture"}]}),
    )?;
    for id in ids {
        let path = format!("worksheets/{}.json", id.as_str().ok_or("id")?);
        let mut rows = workspace::read_record(&work, &path)?;
        for row in rows.as_array_mut().ok_or("worksheet")? {
            row["action"] = json!("preserve");
            row["reviewed"] = json!(true);
        }
        write_json(&work.join(path), &rows)?;
    }
    Ok(work)
}

const RETAINED: &str = "1\n00:00:01,000 --> 00:00:03,000\nRetained dialogue\n";

#[test]
fn f02_workspace_ass_upgrade_validates_before_ad_deletion() -> Check {
    for unsupported in [
        "<notice>https://example.test</notice>",
        r"{\pos(1,2)}https://example.test",
    ] {
        let root = tempfile::tempdir().map_err(|e| e.to_string())?;
        let source = format!(
            "1\n00:00:01,000 --> 00:00:02,000\n{unsupported}\n\n2\n00:00:03,000 --> 00:00:04,000\nRetained dialogue\n"
        );
        let work = reviewed(root.path(), &[&source])?;
        let err = workspace::build(&work, None)
            .expect_err("unsupported markup must fail before publication");
        assert!(
            err.contains("unsupported decoder markup") || err.contains("ASS control syntax"),
            "{err}"
        );
        assert!(!work.join("builds").exists());
        assert_eq!(
            fs::read_to_string(work.join("sources/s0001.srt")).map_err(|e| e.to_string())?,
            source
        );
    }
    Ok(())
}

#[test]
fn f02_workflow_retains_reviewed_dialogue_with_url() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let line = "Open https://example.test to read the manual.";
    let work = reviewed(root.path(), &[&RETAINED.replace("Retained dialogue", line)])?;
    let built = workspace::build(&work, None)?;
    let record = &built["episodes"][0];
    let path = Path::new(built["build"].as_str().ok_or("build path")?)
        .join(record["file"].as_str().ok_or("subtitle path")?);
    let doc = document::parse(&fs::read_to_string(path).map_err(|e| e.to_string())?, "ass")?;
    assert_eq!(
        visible(doc.cues[0].text.clone(), "ass".into())?.replace('\n', " "),
        line
    );
    assert_eq!(record["cues"], 1);
    assert!(record.get("watermarks_removed").is_none());
    Ok(())
}

#[test]
fn f03_alternate_link_cannot_target_removed_watermark() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let base = format!("{RETAINED}\n2\n00:00:04,000 --> 00:00:06,000\nSubtitles by Alice\n");
    let work = reviewed(
        root.path(),
        &[
            &base,
            "1\n00:00:04,000 --> 00:00:06,000\nAlternate source text\n",
        ],
    )?;
    let sheet = "worksheets/s0002.json";
    let mut rows = workspace::read_record(&work, sheet)?;
    rows[0]["action"] = json!("alternate");
    rows[0]["links"] = json!(["s0001:c000002"]);
    rows[0]["note"] = json!("Linked coverage must remain emitted");
    write_json(&work.join(sheet), &rows)?;
    let failed = workspace::build(&work, None).expect_err("link to removed cue must fail");
    assert!(
        failed.contains("emitted") || failed.contains("removed"),
        "{failed}"
    );
    assert!(!work.join("builds").exists());
    rows[0]["links"] = json!(["s0001:c000001"]);
    write_json(&work.join(sheet), &rows)?;
    assert_eq!(workspace::build(&work, None)?["episodes"][0]["cues"], 1);
    Ok(())
}

#[test]
fn f03_review_link_cannot_target_itself() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let work = reviewed(root.path(), &[RETAINED])?;
    let sheet = "worksheets/s0001.json";
    let mut rows = workspace::read_record(&work, sheet)?;
    rows[0]["links"] = json!(["s0001:c000001"]);
    write_json(&work.join(sheet), &rows)?;
    let failed = workspace::build(&work, None).expect_err("self link must fail");
    assert!(failed.contains("self"), "{failed}");
    assert!(!work.join("builds").exists());
    rows[0]["links"] = json!([]);
    write_json(&work.join(sheet), &rows)?;
    assert_eq!(workspace::build(&work, None)?["episodes"][0]["cues"], 1);
    Ok(())
}

#[test]
fn f04_translator_mode_keeps_overlapping_primary_read_order_and_styles() -> Check {
    let mut original = ass(r"{\pos(160,90)}{\1c&H0000FF&}VISIBLE")?;
    let mut earlier = original.cues[0].clone();
    earlier.id = "c000002".into();
    earlier.start = 0;
    earlier.text = r"{\pos(160,90)}{\1c&H00FF00&}VISIBLE".into();
    original.cues.push(earlier);
    let style = original.styles["Default"].join(",");
    let (converted, stats) = conversion::translator_ass(
        &original,
        &[],
        true,
        &Options {
            strip_watermarks: false,
            style: Some(&style),
            ..options()
        },
    )?;
    let rendered = document::serialize(&converted, &converted.cues, false)?;
    let result = document::parse(&rendered, "ass")?;
    assert_eq!(
        result
            .cues
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>(),
        [
            original.cues[0].text.as_str(),
            original.cues[1].text.as_str()
        ]
    );
    assert_eq!(
        result.cues.iter().map(|c| c.start).collect::<Vec<_>>(),
        [1000, 0]
    );
    assert_eq!(stats["placeholder_slots"], 0);
    assert_eq!(original.cues[0].start, 1000);
    Ok(())
}

macro_rules! isolates {
    ($($name:ident: ($kind:literal,$open:literal,$close:literal,$break:literal)),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let mixed = "NASA اینجاست، ساعت ۱۲:۳۰!";
            let body = format!("{}{}{}{}English only", $open, mixed, $close, $break);
            let expected = format!("{}{RLM}{RLI}{mixed}{PDI}{}{}English only", $open, $close, $break);
            let result = markup::rtl(body, $kind.into(), "rtl".into(), false)?;
            assert_eq!(result, expected);
            assert_eq!(markup::rtl(result.clone(), $kind.into(), "rtl".into(), false)?, expected);
            assert_eq!(visible(result.clone(), $kind.into())?, format!("{mixed}\nEnglish only"));
            markup::validate_bidi(result, $kind.into())
        }
    )+};
}

isolates! {
    fr019_srt_bold_remains_outside_isolate: ("srt", "<b>", "</b>", "\n"),
    fr019_srt_italic_remains_outside_isolate: ("srt", "<i>", "</i>", "\n"),
    fr019_srt_nested_emphasis_is_not_reordered: ("srt", "<b><i>", "</i></b>", "\n"),
    fr019_ass_bold_remains_outside_isolate: ("ass", r"{\b1}", r"{\b0}", r"\N"),
    fr019_ass_italic_remains_outside_isolate: ("ass", r"{\i1}", r"{\i0}", r"\N"),
    fr019_ass_nested_emphasis_is_not_reordered: ("ass", r"{\b1}{\i1}", r"{\i0}{\b0}", r"\N"),
}
