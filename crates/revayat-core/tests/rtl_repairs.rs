use revayat_core::{markup, text_pipeline};

fn anchor(input: &str) -> Result<String, String> {
    text_pipeline::anchor_rtl_line(input.into(), Some("ass".into()), Some("rtl".into()))
}

#[test]
fn anchors_and_repairs_imported_lines() -> Result<(), String> {
    for (input, expected) in [
        ("سلام", "سلام"),
        ("6تا آبجو معاوضم کرد.", "6تا آبجو معاوضم کرد."),
        ("NASA اینجاست!", "NASA اینجاست!"),
        ("- سلام!", "- سلام!"),
        ("میدی \u{202b}یا من", "میدی یا من"),
        ("\u{202b}\u{200f}سلام\u{200f}\u{202c}", "سلام"),
        (".سلام", "سلام."),
        ("،اگه ماموریت داری", "اگه ماموریت داری،"),
        ("?!سلام", "سلام?!"),
        ("...کوتاهی کنی", "کوتاهی کنی..."),
        ("…کوتاهی کنی", "کوتاهی کنی…"),
        ("...ولی رفتم.", "...ولی رفتم."),
    ] {
        let result = anchor(input)?;
        assert_eq!(
            result,
            format!("\u{200f}\u{2067}{expected}\u{2069}"),
            "{input:?}"
        );
        assert_eq!(anchor(&result)?, result);
    }
    let input = r"{\i1}سلام!{\i0}";
    let expected = "{\\i1}\u{200f}\u{2067}سلام!\u{2069}{\\i0}";
    assert_eq!(anchor(input)?, expected);
    assert_eq!(
        markup::rtl(input.into(), "ass".into(), "rtl".into(), false)?,
        expected
    );
    Ok(())
}

#[test]
fn repairs_intra_word_marks_only_in_prose_and_keeps_commas_off_continuation_starts()
-> Result<(), String> {
    let result = text_pipeline::normalize_persian_zwnj("می\u{200f}دونی اون\u{200f}وقت `می\u{200f}دونی` {می\u{200f}دونی} https://example.org/می\u{200f}دونی".into())?;
    assert_eq!(
        result,
        "می‌دونی اون‌وقت `می\u{200f}دونی` {می\u{200f}دونی} https://example.org/می\u{200f}دونی"
    );
    let wrapped = text_pipeline::wrap_subtitle_lines(
        "تو برای من خبر آوردی ، پیام مهمی بود".into(),
        Some(19),
        Some(2),
    )?;
    let (first, second) = wrapped.split_once(r"\N").expect("fixture wraps");
    assert!(
        first.ends_with('،') || second.contains("آوردی ،"),
        "{wrapped:?}"
    );
    assert!(!second.trim_start().starts_with('،'), "{wrapped:?}");
    Ok(())
}
