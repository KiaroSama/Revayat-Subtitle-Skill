use revayat_core::{markup, text_pipeline};

#[test]
fn isolates_latin_first_persian_without_touching_ass_overrides() -> Result<(), String> {
    let body = "NASA \u{0627}\u{06cc}\u{0646}\u{062c}\u{0627}\u{0633}\u{062a}";
    let input = format!(r"{{\i1}}{body}{{\i0}}");
    let expected = format!(
        r"{{\i1}}{}{}{}{}{{\i0}}",
        '\u{200f}', '\u{2067}', body, '\u{2069}'
    );
    let result = markup::rtl(input.clone(), "ass".into(), "rtl".into(), false)?;
    assert_eq!(result, expected);
    assert_eq!(
        markup::rtl(result, "ass".into(), "rtl".into(), false)?,
        expected
    );
    let anchor =
        |text| text_pipeline::anchor_rtl_line(text, Some("ass".into()), Some("rtl".into()));
    let anchored = anchor(input)?;
    assert_eq!(anchored, expected);
    assert_eq!(anchor(anchored)?, expected);
    Ok(())
}

#[test]
fn isolates_mixed_srt_lines_without_wrapping_the_tags() -> Result<(), String> {
    let body = "NASA \u{0627}\u{06cc}\u{0646}\u{062c}\u{0627}\u{0633}\u{062a}!";
    let input = format!("<i>{body}</i>\nplain English");
    let expected = format!("<i>\u{200f}\u{2067}{body}\u{2069}</i>\nplain English");
    assert_eq!(
        markup::rtl(input.clone(), "srt".into(), "rtl".into(), false)?,
        expected
    );
    assert_eq!(
        text_pipeline::anchor_rtl_line(input, Some("srt".into()), Some("rtl".into()))?,
        expected
    );
    Ok(())
}
