use proptest::prelude::*;
use proptest::test_runner::{RngSeed, TestCaseError};
use revayat_core::markup::{rtl, uncomment, validate_bidi};
use revayat_core::persian_lint;
use revayat_core::subtitle_formats::{SubtitleCue, SubtitleDocument, document, visible};
use revayat_core::workflow::review::reviewed_cues;
use serde_json::json;
use std::time::{Duration, Instant};

fn config(cases: u32) -> ProptestConfig {
    ProptestConfig {
        cases,
        rng_seed: RngSeed::Fixed(0x1706_82aa),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

fn subtitle_text() -> impl Strategy<Value = String> {
    (
        "[سلامدنیاabcXYZ012۵۶۷]",
        "[سلامدنیا abcXYZ012۵۶۷<>()!?‌‏]{0,98}",
        "[سلامدنیاabcXYZ012۵۶۷]",
    )
        .prop_map(|(first, rest, last)| format!("{first}{rest}{last}"))
}

fn cue(start: i64, text: String) -> SubtitleCue {
    SubtitleCue {
        id: "c000001".into(),
        start,
        end: start + 1000,
        text,
        ..SubtitleCue::default()
    }
}

fn srt(cues: Vec<SubtitleCue>) -> SubtitleDocument {
    SubtitleDocument {
        kind: "srt".into(),
        cues,
        chronological: true,
        ..SubtitleDocument::default()
    }
}

fn review(text: &str) -> Result<(SubtitleDocument, Vec<SubtitleCue>), String> {
    let doc = srt(vec![cue(1000, "Source".into())]);
    let row = json!({
        "id": "c000001", "source_text": "Source", "start_ms": 1000, "end_ms": 2000,
        "action": "edit", "text": text, "reviewed": true, "links": [], "note": "",
        "structure_note": "Line layout", "timing_note": ""
    });
    let (cues, _) = reviewed_cues(&doc, &[row], "en")?;
    Ok((doc, cues))
}

proptest! {
    #![proptest_config(config(100))]

    #[test]
    fn logical_text_and_rtl_idempotence(text in subtitle_text()) {
        let normalized = rtl(text.clone(), "srt".into(), "rtl".into(), false).map_err(TestCaseError::fail)?;
        let before = visible(text, "srt".into()).map_err(TestCaseError::fail)?;
        let after = visible(normalized.clone(), "srt".into()).map_err(TestCaseError::fail)?;
        prop_assert_eq!(before, after);
        prop_assert_eq!(&normalized, &rtl(normalized.clone(), "srt".into(), "rtl".into(), false).map_err(TestCaseError::fail)?);
    }

    #[test]
    fn srt_roundtrip_keeps_sorted_content(values in prop::collection::vec((0..=100_000_i64, subtitle_text()), 1..=12)) {
        let cues: Vec<_> = values.into_iter().enumerate().map(|(i, (start, text))| SubtitleCue {
            id: format!("c{:06}", i + 1),
            ..cue(start, text)
        }).collect();
        let doc = srt(cues);
        let encoded = document::serialize(&doc, &doc.cues, false).map_err(TestCaseError::fail)?;
        let actual = document::parse(&encoded, "srt").map_err(TestCaseError::fail)?;
        let mut expected: Vec<_> = doc.cues.iter().map(|c| (c.start, c.end, c.text.as_str())).collect();
        expected.sort_by_key(|c| c.0);
        prop_assert_eq!(actual.cues.iter().map(|c| (c.start, c.end, c.text.as_str())).collect::<Vec<_>>(), expected);
    }

    #[test]
    fn ordinary_comments_coalesce_exact_literal_prose(left in "[abcXYZسلام012]{1,20}", right in "[abcXYZسلام012]{1,20}") {
        let actual = uncomment(format!("{left}<!--authored-->{right}"), "srt".into()).map_err(TestCaseError::fail)?;
        prop_assert_eq!(actual, left + &right);
    }

    #[test]
    fn removed_boundary_never_activates_tag_family(tag in prop::sample::select(vec!["b", "/b", "br", "small", "foo"])) {
        let result = uncomment(format!("A<<!--authored-->{tag}>B"), "srt".into());
        prop_assert!(matches!(result, Err(ref error) if error.contains("activat")), "{result:?}");
    }

    #[test]
    fn single_reviewed_newline_has_independent_literal_roundtrip(left in "[abcXYZ012]{1,20}", right in "[abcXYZ012]{1,20}", newline in prop::sample::select(vec!["\n", "\r\n", "\r"])) {
        let (doc, cues) = review(&format!("{left}{newline}{right}")).map_err(TestCaseError::fail)?;
        let expected = format!("{left}\n{right}");
        prop_assert_eq!(&cues[0].text, &expected);
        let serialized = document::serialize(&doc, &cues, false).map_err(TestCaseError::fail)?;
        let parsed = document::parse(&serialized, "srt").map_err(TestCaseError::fail)?;
        prop_assert_eq!(&parsed.cues[0].text, &expected);
    }

    #[test]
    fn mixed_direction_has_independent_ltr_wrapper_oracle(persian in "س[سلامدنیا]{0,12}", latin in "a[a-z]{0,12}") {
        let (_, cues) = review(&format!("{persian}\n{latin}")).map_err(TestCaseError::fail)?;
        let expected = format!("\u{200e}{persian}\u{200e}\n{latin}");
        prop_assert_eq!(&cues[0].text, &expected);
    }

    #[test]
    fn ass_escapes_and_drawing_transitions_have_literal_oracles(text in prop::sample::select(vec![r"\{literal}", r"x\hY", r"{\p1}m 0 0 l 1 1{note}{\p0}X"])) {
        let actual = uncomment(text.into(), "ass".into()).map_err(TestCaseError::fail)?;
        prop_assert_eq!(actual, text.replace("{note}", "{}"));
    }

    #[test]
    fn supported_formatting_preserves_significant_interior_whitespace(prose in prop::sample::select(vec![" A ", "\tA\t", "A  B"]), tag in prop::sample::select(vec!["b", "i", "u", "s"])) {
        let original = format!("<{tag}>{prose}<!--note-->Z</{tag}>");
        let expected = format!("<{tag}>{prose}Z</{tag}>");
        prop_assert_eq!(uncomment(original, "srt".into()).map_err(TestCaseError::fail)?, expected);
    }

    #[test]
    fn literal_comment_oracle_detects_a_shifted_boundary(note in "[a-z]{1,20}") {
        let original = format!("A<!--{note}-->Z");
        let shifted = format!("A<!--{note}Z-->");
        let expected = "AZ";
        prop_assert_eq!(uncomment(original, "srt".into()).map_err(TestCaseError::fail)?, expected);
        prop_assert_ne!(uncomment(shifted, "srt".into()).map_err(TestCaseError::fail)?, expected);
    }

    #[test]
    fn srt_cleanup_is_idempotent(left in "[a-zسلام]{1,20}", tag in prop::sample::select(vec!["b", "i", "u"])) {
        let input = format!("<{tag}>{left}<!--note--> Z</{tag}>");
        let once = uncomment(input, "srt".into()).map_err(TestCaseError::fail)?;
        prop_assert_eq!(&once, &uncomment(once.clone(), "srt".into()).map_err(TestCaseError::fail)?);
        prop_assert_eq!(once, format!("<{tag}>{left} Z</{tag}>"));
    }

    #[test]
    fn ass_rtl_preserves_override_positions_and_is_idempotent(latin in "[A-Z]{1,12}", persian in "[سلامدنیا]{1,12}") {
        let input = format!(r"{{\i1}}{latin} {persian}{{\i0}}\N{{\b1}}{persian}{{\b0}}");
        let expected = format!(r"{{\i1}}{rtl_start}{latin} {persian}{pdi}{{\i0}}\N{{\b1}}{mark}{persian}{mark}{{\b0}}", rtl_start = "\u{200f}\u{2067}", pdi = '\u{2069}', mark = '\u{200f}');
        let actual = rtl(input, "ass".into(), "rtl".into(), false).map_err(TestCaseError::fail)?;
        prop_assert_eq!(&actual, &expected);
        prop_assert_eq!(&actual, &rtl(actual.clone(), "ass".into(), "rtl".into(), false).map_err(TestCaseError::fail)?);
    }

    #[test]
    fn rtl_wraps_mixed_latin_lead_with_one_rli_pdi_pair(latin in "[A-Za-z]{1,12}", persian in "[سلامدنیا]{1,12}") {
        let input = format!("<b>{latin} {persian}؟</b>");
        let expected = format!("<b>\u{200f}\u{2067}{latin} {persian}؟\u{2069}</b>");
        let actual = rtl(input, "srt".into(), "rtl".into(), false).map_err(TestCaseError::fail)?;
        prop_assert_eq!(&actual, &expected);
        validate_bidi(actual, "srt".into()).map_err(TestCaseError::fail)?;
    }

    #[test]
    fn utf8_srt_roundtrip_keeps_markup_and_scalar_sequence(word in "[فارسی猫🦀é]{1,24}", start in 0..90_000_i64) {
        let text = format!("<i>{word}</i> سلام ۱۲۳");
        let doc = srt(vec![cue(start, text.clone())]);
        let serialized = document::serialize(&doc, &doc.cues, false).map_err(TestCaseError::fail)?;
        let recovered = String::from_utf8(serialized.into_bytes())?;
        let parsed = document::parse(&recovered, "srt").map_err(TestCaseError::fail)?;
        prop_assert_eq!(parsed.cues[0].text.as_bytes(), text.as_bytes());
        prop_assert_eq!((parsed.cues[0].start, parsed.cues[0].end), (start, start + 1000));
    }

    #[test]
    fn comment_cleanup_preserves_literal_nuls_and_multibyte_prose(left in "[Aéسلام]{1,12}", right in "[B猫دنیا]{1,12}", note in "[a-z0-9]{1,10}") {
        let input = format!("{left}\0<!--{note}-->{right}\0");
        let actual = uncomment(input, "srt".into()).map_err(TestCaseError::fail)?;
        prop_assert_eq!(actual, format!("{left}\0{right}\0"));
    }

    #[test]
    fn persian_lint_is_idempotent_around_protected_nul(code in "[a-z0-9كي]{1,15}", note in "[A-Z]{1,12}") {
        let input = format!("كتاب ها \0`{code}\0` {note}\0 مي رود");
        let expected = format!("کتاب‌ها \0`{code}\0` {note}\0 می‌رود");
        let first = persian_lint::normalize(json!(input), "keep".into()).map_err(TestCaseError::fail)?;
        prop_assert_eq!(&first["$tuple"][0], &json!(expected));
        let second = persian_lint::normalize(first["$tuple"][0].clone(), "keep".into()).map_err(TestCaseError::fail)?;
        prop_assert_eq!(second, json!({"$tuple": [expected, {}]}));
    }

    #[test]
    fn ass_roundtrip_keeps_drawings_and_override_tags(x in 0..640, y in 0..480, ending in prop::sample::select(vec!["\n", "\r\n", "\r"])) {
        let text = format!(r"{{\p1}}m 0 0 l {x} {y}{{\p0}}{{\i1}}سلام, hello{{\i0}}");
        let source = format!("[Script Info]\nScriptType: v4.00+\n\n[V4+ Styles]\nFormat: Name\nStyle: Default\n\n[Events]\nFormat: Layer, Start, End, Style, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,{text}\n").replace('\n', ending);
        let doc = document::parse(&source, "ass").map_err(TestCaseError::fail)?;
        let serialized = document::serialize(&doc, &doc.cues, false).map_err(TestCaseError::fail)?;
        let parsed = document::parse(&serialized, "ass").map_err(TestCaseError::fail)?;
        prop_assert_eq!(&parsed.cues[0].text, &text);
        prop_assert_eq!((parsed.cues[0].start, parsed.cues[0].end), (1000, 2000));
    }
}

proptest! {
    #![proptest_config(config(6))]

    #[test]
    fn dense_comment_cleanup_has_bounded_growth(repeats in 768..1024_usize, note in "[a-z]{4,10}") {
        let segment = format!("سلام<!--{note}-->A");
        let small = segment.repeat(repeats);
        let large = segment.repeat(repeats * 16);
        let start = Instant::now();
        let actual = uncomment(small, "srt".into()).map_err(TestCaseError::fail)?;
        let small_elapsed = start.elapsed();
        prop_assert_eq!(actual, "سلامA".repeat(repeats));
        let start = Instant::now();
        let actual = uncomment(large, "srt".into()).map_err(TestCaseError::fail)?;
        let large_elapsed = start.elapsed();
        prop_assert_eq!(actual, "سلامA".repeat(repeats * 16));
        // Slack covers scheduling and timer noise; this guards growth, not exact complexity.
        prop_assert!(large_elapsed < small_elapsed * 40 + Duration::from_millis(300), "{small_elapsed:?} -> {large_elapsed:?}");
    }
}
