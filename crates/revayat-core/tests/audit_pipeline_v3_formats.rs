//! Authored regressions for the Rust rewrite's subtitle preservation boundaries.

use revayat_core::subtitle_formats::{
    conversion::{self, Options},
    document::{self, Cue, Document},
};
use revayat_core::{lookup, markup, persian_lint, subtitle_formats, text_pipeline};
use serde_json::json;

type Check = Result<(), String>;

fn srt(text: &str, start: i64, end: i64) -> Document {
    Document {
        kind: "srt".into(),
        cues: vec![Cue {
            id: "c000001".into(),
            start,
            end,
            text: text.into(),
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn options() -> Options<'static> {
    Options {
        direction: "ltr",
        fix_zwnj: false,
        wrap_lines: false,
        strip_watermarks: false,
        ..Default::default()
    }
}

#[test]
fn srt_accepts_complete_blank_line_runs_with_each_line_ending() -> Check {
    for newline in ["\n", "\r\n", "\r"] {
        for separator in ["\n\n", "\n\n\n", "\n\n\n\n", "\n\n\n\n\n", "\n \t\n\t\n"] {
            let input = format!("1\n00:00:00,000 --> 00:00:01,000\nFirst{separator}2\n00:00:02,000 --> 00:00:03,000\nSecond\n").replace('\n', newline);
            let parsed = document::parse(&input, "srt")?;
            assert_eq!(
                parsed
                    .cues
                    .iter()
                    .map(|c| c.text.as_str())
                    .collect::<Vec<_>>(),
                ["First", "Second"]
            );
        }
    }
    assert!(
        document::parse(
            "1\n00:00:00,000 --> 00:00:01,000\nFirst\n\nnot a cue",
            "srt"
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn comment_cleanup_cannot_create_a_new_hidden_comment() -> Check {
    for source in [
        "A<!<!--note-->--hidden-->B",
        "A<!!--x--><!--note-->B<!<!--note-->--hidden-->C",
    ] {
        let error = markup::uncomment(source.into(), "srt".into())
            .expect_err("new comments must be refused");
        assert!(error.contains("activate literal markup"), "{error}");
    }
    for (source, expected) in [
        ("A<!--note-->B", "AB"),
        ("A<!--note-->B<!--second-->C", "ABC"),
        ("A<!--unfinished", "A<!--unfinished"),
    ] {
        assert_eq!(markup::uncomment(source.into(), "srt".into())?, expected);
    }
    Ok(())
}

fn complete_outer_isolate(text: &str) -> bool {
    if !text.starts_with("\u{200f}\u{2067}") || !text.ends_with('\u{2069}') {
        return false;
    }
    let mut depth = 0;
    for (offset, ch) in text.char_indices() {
        match ch {
            '\u{2066}'..='\u{2068}' => depth += 1,
            '\u{2069}' => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
                if depth == 0 && offset + ch.len_utf8() != text.len() {
                    return false;
                }
            }
            _ => (),
        }
    }
    depth == 0
}

#[test]
fn sibling_isolates_do_not_replace_a_complete_paragraph_scope() -> Check {
    let source = "\u{200f}\u{2067}OpenAI\u{2069} سلام \u{2067}فارسی\u{2069}";
    for kind in ["ass", "srt"] {
        let output = markup::rtl(source.into(), kind.into(), "rtl".into(), false)?;
        assert!(complete_outer_isolate(&output), "{kind}: {output:?}");
        assert_eq!(
            markup::rtl(output.clone(), kind.into(), "rtl".into(), false)?,
            output
        );
        let output =
            text_pipeline::anchor_rtl_line(source.into(), Some(kind.into()), Some("rtl".into()))?;
        assert!(complete_outer_isolate(&output), "{kind}: {output:?}");
        assert_eq!(
            text_pipeline::anchor_rtl_line(output.clone(), Some(kind.into()), Some("rtl".into()))?,
            output
        );
    }
    Ok(())
}

#[test]
fn actual_outer_isolates_with_nested_scopes_remain_idempotent() -> Check {
    let source = "\u{200f}\u{2067}OpenAI سلام \u{2066}ABC\u{2069} تمام\u{2069}";
    for kind in ["ass", "srt"] {
        assert_eq!(
            markup::rtl(source.into(), kind.into(), "rtl".into(), false)?,
            source
        );
        assert_eq!(
            text_pipeline::anchor_rtl_line(source.into(), Some(kind.into()), Some("rtl".into()))?,
            source
        );
    }
    Ok(())
}

#[test]
fn normalizer_preserves_the_actual_first_close_ass_override() -> Check {
    let source = r"{\fnمي روم{X}مي روم";
    let expected = "{\\fnمي روم{X}می‌روم";
    assert_eq!(
        text_pipeline::normalize_persian_zwnj(source.into())?,
        expected
    );
    let (mut native, _) = conversion::convert(&srt("Primary", 0, 1000), &options())?;
    native.cues[0].text = source.into();
    let (converted, _) = conversion::convert(
        &native,
        &Options {
            direction: "rtl",
            fix_zwnj: true,
            ..options()
        },
    )?;
    assert!(converted.cues[0].text.starts_with(r"{\fnمي روم{X}"));
    assert!(converted.cues[0].text.contains("می‌روم"));
    assert_eq!(
        document::ass_commands(r"{\fnمي روم{X}")?,
        document::ass_commands(&converted.cues[0].text)?
    );
    Ok(())
}

#[test]
fn normalization_does_not_repeatedly_scan_unterminated_ass_openers() -> Check {
    let source = format!("{}مي روم", "{".repeat(100_000));
    assert_eq!(
        text_pipeline::normalize_persian_zwnj(source)?,
        format!("{}می‌روم", "{".repeat(100_000))
    );
    Ok(())
}

#[test]
fn normalization_preserves_comments_and_tags_around_nested_literal_braces() -> Check {
    for protected in [
        "<!-- مي {x} روم -->",
        r"{\fnمي <!-- {x} ",
        r#"<i title="مي {x} روم">"#,
    ] {
        assert_eq!(
            text_pipeline::normalize_persian_zwnj(protected.into())?,
            protected
        );
    }
    assert_eq!(
        text_pipeline::normalize_persian_zwnj(r"\{مي روم}".into())?,
        "\\{می‌روم}"
    );
    Ok(())
}

#[test]
fn generated_style_aliases_have_one_serializable_reference() -> Check {
    for name in [
        "Default",
        "default",
        "DEFAULT",
        "*default",
        " Default",
        "Custom",
        "\u{a0}Custom",
    ] {
        let raw = format!(
            "{name},{}",
            document::DEFAULT_STYLE
                .split_once(',')
                .ok_or("default fields")?
                .1
        );
        let settings = Options {
            style: Some(&raw),
            ..options()
        };
        let expected = if name
            .trim_start_matches([' ', '*'])
            .eq_ignore_ascii_case("default")
        {
            "Default"
        } else {
            name
        };
        let (converted, _) = conversion::convert(&srt("Primary", 0, 1000), &settings)?;
        assert_eq!(
            converted
                .styles
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            [expected]
        );
        assert_eq!(converted.cues[0].fields["style"], expected);
        document::parse(
            &document::serialize(&converted, &converted.cues, false)?,
            "ass",
        )?;
        let (translated, report) = conversion::translator_ass(
            &srt("Primary", 0, 1000),
            &[("Donor".into(), srt("Donor", 4000, 5000))],
            true,
            &settings,
        )?;
        assert_eq!(translated.cues[1].fields["style"], expected);
        assert_eq!(report["style"], expected);
        document::parse(
            &document::serialize(&translated, &translated.cues, false)?,
            "ass",
        )?;
    }
    Ok(())
}

#[test]
fn requested_title_is_inserted_without_an_existing_script_info_section() -> Check {
    let (mut native, _) = conversion::convert(&srt("Primary", 0, 1000), &options())?;
    native
        .sections
        .retain(|(name, _)| !name.eq_ignore_ascii_case("[Script Info]"));
    let original = native.clone();
    let (converted, _) = conversion::convert(
        &native,
        &Options {
            title: "Authored title",
            ..options()
        },
    )?;
    assert_eq!(
        converted
            .sections
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("[Script Info]"))
            .count(),
        1
    );
    let output = document::serialize(&converted, &converted.cues, false)?;
    assert!(output.contains("Title: Authored title"));
    assert_eq!(native, original);
    document::parse(&output, "ass")?;
    Ok(())
}

#[test]
fn effective_timing_rejects_unsupported_formats_in_both_public_apis() -> Check {
    for kind in ["", "vtt", "ASS", "srt "] {
        assert!(document::effective_times(1, 20, kind).is_err(), "{kind:?}");
        assert!(
            subtitle_formats::effective_times(1, 20, kind.into()).is_err(),
            "{kind:?}"
        );
    }
    assert_eq!(document::effective_times(1, 20, "srt")?, (1, 20));
    assert_eq!(document::effective_times(1, 20, "ass")?, (0, 20));
    Ok(())
}

#[test]
fn reverse_lookup_requires_a_nonempty_first_translation() -> Check {
    let rows = json!([
        ["empty", "/ سلام", ""],
        ["spaces", " \t / سلام", ""],
        ["ordinary", "سلام / درود", ""]
    ]);
    assert_eq!(lookup::find_fa(rows.clone(), json!("خداحافظ"))?, json!([]));
    assert_eq!(lookup::find_fa(rows.clone(), json!("درود"))?, json!([]));
    assert_eq!(
        lookup::find_fa(rows, json!("سلام"))?,
        json!([{"$tuple": ["ordinary", "سلام / درود", ""]}])
    );
    Ok(())
}

#[test]
fn persian_linter_keeps_ambiguous_wet_and_locates_only_unprotected_hints() -> Check {
    assert_eq!(
        persian_lint::normalize(json!("زمینِ تر بود"), "keep".into())?["$tuple"][0],
        "زمینِ تر بود"
    );
    assert_eq!(
        persian_lint::suggestions(json!("    میرود code"), false)?,
        json!([])
    );
    assert_eq!(persian_lint::suggestions(json!("میز"), false)?, json!([]));
    let hints = persian_lint::suggestions(json!("    protected\n\nمیرود پایان"), false)?;
    assert_eq!(hints.as_array().ok_or("hints")?.len(), 1);
    assert_eq!(hints[0]["type"], "maybe_glued_mi");
    assert!(
        hints[0]["context"]
            .as_str()
            .ok_or("context")?
            .contains("میرود")
    );
    Ok(())
}
