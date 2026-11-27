//! Subtitle prose, drawing spans, wrapping, and watermark classification.
#![forbid(unsafe_code)]

use fancy_regex::Regex as FancyRegex;
use regex::Regex;
use std::collections::HashSet;

macro_rules! pattern {
    ($engine:ty, $source:expr) => {{
        static PATTERN: std::sync::OnceLock<$engine> = std::sync::OnceLock::new();
        PATTERN.get_or_init(|| {
            let source = $source;
            // INFALLIBLE: All patterns are fixed by this module.
            <$engine>::new(source.as_ref()).expect("valid subtitle pattern")
        })
    }};
}

use crate::markup::{
    drawing_in as drawing_mode, python_space as py_space, tag_fits as srt_tag_fits,
};
mod lines;
mod prose;
mod tokens;

use lines::{anchor, wrap};

const ZWNJ: char = '\u{200c}';
const RLE: char = '\u{202b}';
const PDF: char = '\u{202c}';
const BIDI: &str = "\u{061c}\u{200e}\u{200f}\u{202a}\u{202b}\u{202c}\u{202d}\u{202e}\u{2066}\u{2067}\u{2068}\u{2069}";
const ASS: &str = r"\{";
const HTML: &str = r"</?[A-Za-z][^<>]*>";
const WORD: &str = r"\p{L}\p{N}_";

fn regex_error(error: fancy_regex::Error) -> String {
    format!("RuntimeError: subtitle regex failed: {error}")
}

fn tags() -> &'static FancyRegex {
    pattern!(FancyRegex, format!("{ASS}|{HTML}|<!--"))
}

fn alignment() -> &'static Regex {
    pattern!(Regex, r"\A\{\\?(?i-u:an)[1-9]\}\z")
}

fn srt_break(tag: &str) -> bool {
    pattern!(Regex, r"^</?(?i-u:br)/?(?: [^<>]*)?>\z").is_match(tag)
        && tag.chars().count() <= if tag.starts_with("</") { 130 } else { 129 }
}

fn srt_color() -> &'static Regex {
    pattern!(
        Regex,
        r##"\A(?i-u:<font) +(?i-u:color) *= *(?:"#([0-9A-Fa-f]{6})"|'#([0-9A-Fa-f]{6})'|#([0-9A-Fa-f]{6})) *>\z"##
    )
}

fn simple_srt_markup() -> &'static Regex {
    pattern!(Regex, r"\A(?i-u:</?[bisu]>|</font>)\z")
}

fn segments<'a>(
    text: &'a str,
    engine: &'static FancyRegex,
) -> impl Iterator<Item = Result<(&'a str, &'a str), String>> {
    let (mut cursor, mut search, mut missing_end, mut output) = (0, 0, false, Vec::new());
    let (mut missing_ass_end, mut drawing) = (false, false);
    loop {
        let found = match engine.find_from_pos(text, search) {
            Ok(Some(found)) => found,
            Ok(None) => break,
            Err(error) => {
                output.push(Err(regex_error(error)));
                return output.into_iter();
            }
        };
        search = found.end();
        if found.as_str() == "{" {
            if missing_ass_end
                || (!drawing && found.start() > 0 && text.as_bytes()[found.start() - 1] == b'\\')
            {
                continue;
            }
            if let Some(end) = text[search..].find('}') {
                search += end + 1;
                drawing = match drawing_mode(&text[found.start()..search], drawing) {
                    Ok(next) => next,
                    Err(error) => {
                        output.push(Err(error));
                        return output.into_iter();
                    }
                };
            } else {
                // A later opener cannot have a closer after this failed search.
                missing_ass_end = true;
                continue;
            }
        } else if found.as_str() == "<!--" {
            // Once a comment has no terminator, later openers cannot complete
            // one either. Preserve the suffix as prose without rescanning EOF.
            if !missing_end && let Some(end) = text[search..].find("-->") {
                search += end + 3;
            } else {
                missing_end = true;
                continue;
            }
        }
        output.push(Ok((
            &text[cursor..found.start()],
            &text[found.start()..search],
        )));
        cursor = search;
    }
    output.push(Ok((&text[cursor..], "")));
    output.into_iter()
}

pub fn normalize_persian_zwnj(text: String) -> Result<String, String> {
    normalize_persian_zwnj_with_word_class(text, None)
}

pub fn normalize_persian_zwnj_with_word_class(
    text: String,
    runtime_word_class: Option<&str>,
) -> Result<String, String> {
    prose::normalize(&text, runtime_word_class)
}

pub fn python_word_class(classes: &[(char, bool)]) -> Option<String> {
    let word = pattern!(Regex, format!("\\A[{WORD}]\\z"));
    let (mut added, mut removed) = (Vec::new(), Vec::new());
    for &(c, python_word) in classes {
        if python_word != word.is_match(&c.to_string()) {
            if python_word {
                added.push(c);
            } else {
                removed.push(c);
            }
        }
    }
    if added.is_empty() && removed.is_empty() {
        return None;
    }
    added.sort_unstable();
    removed.sort_unstable();
    let escapes = |chars: Vec<char>| {
        chars
            .into_iter()
            .map(|c| format!("\\u{{{:X}}}", u32::from(c)))
            .collect::<String>()
    };
    // Override only characters whose Unicode classification differs. Normal
    // cues keep the cached patterns; rare version differences compile locally.
    let base = if removed.is_empty() {
        WORD.to_owned()
    } else {
        format!("[[{WORD}]--[{}]]", escapes(removed))
    };
    Some(format!("{base}{}", escapes(added)))
}

pub fn watermark_reason(text: String) -> Result<String, String> {
    watermark_reason_with_kind(text, "srt")
}

pub fn watermark_reason_with_kind(text: String, kind: &str) -> Result<String, String> {
    prose::watermark(&text, kind).map(str::to_owned)
}

pub fn prepare_watermark_text(text: String) -> Result<String, String> {
    prepare_watermark_text_with_kind(text, "srt")
}

pub fn prepare_watermark_text_with_kind(text: String, kind: &str) -> Result<String, String> {
    prose::watermark_text(&text, kind)
}

pub fn watermark_reason_prepared(
    plain: String,
    runtime_word_class: Option<&str>,
) -> Result<String, String> {
    prose::watermark_prepared(&plain, runtime_word_class).map(str::to_owned)
}

pub fn is_watermark_cue(text: String) -> Result<bool, String> {
    is_watermark_cue_with_kind(text, "srt")
}

pub fn is_watermark_cue_with_kind(text: String, kind: &str) -> Result<bool, String> {
    Ok(!prose::watermark(&text, kind)?.is_empty())
}

pub fn convert_srt_markup_to_ass(text: String) -> Result<String, String> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut output = String::with_capacity(text.len());
    let mut colors = Vec::new();
    let color = srt_color();
    let simple = simple_srt_markup();
    for segment in segments(&text, tags()) {
        let (prose, tag) = segment?;
        output.push_str(&prose.replace('\n', r"\N"));
        if alignment().is_match(tag) {
            output.push_str(r"{\an");
            output.push_str(&tag[tag.len() - 2..]);
        } else if !tag.starts_with('<') || !srt_tag_fits(tag) {
            output.push_str(tag);
        } else if srt_break(tag) {
            output.push_str(r"\N");
        } else if let Some(rgb) = color
            .captures(tag)
            .and_then(|m| m.iter().skip(1).flatten().next())
        {
            let rgb = rgb.as_str().to_ascii_uppercase();
            let value = format!("&H{}{}{}&", &rgb[4..], &rgb[2..4], &rgb[..2]);
            output.push_str(r"{\c");
            output.push_str(&value);
            output.push('}');
            colors.push(value);
        } else if simple.is_match(tag) {
            if tag.eq_ignore_ascii_case("</font>") {
                colors.pop();
                output.push_str(r"{\c");
                if let Some(value) = colors.last() {
                    output.push_str(value);
                }
            } else {
                output.push_str("{\\");
                output.push((tag.as_bytes()[tag.len() - 2] as char).to_ascii_lowercase());
                output.push(if tag.starts_with("</") { '0' } else { '1' });
            }
            output.push('}');
        } else {
            output.push_str(tag);
        }
    }
    Ok(output)
}

pub fn wrap_subtitle_lines(
    text: String,
    max_chars: Option<i64>,
    max_lines: Option<i64>,
) -> Result<String, String> {
    let max_chars = max_chars.ok_or_else(|| {
        "TypeError: '<' not supported between instances of 'NoneType' and 'int'".to_string()
    })?;
    if max_chars < 1 {
        return Err("ValueError: Line wrapping requires positive max_chars and max_lines".into());
    }
    let max_lines = max_lines.ok_or_else(|| {
        "TypeError: '<' not supported between instances of 'NoneType' and 'int'".to_string()
    })?;
    wrap(text, max_chars, max_lines, "ass")
}

pub fn anchor_rtl_line(
    text: String,
    kind: Option<String>,
    direction: Option<String>,
) -> Result<String, String> {
    anchor_rtl_line_with_letters(text, kind, direction, None)
}

pub fn anchor_rtl_line_with_letters(
    text: String,
    kind: Option<String>,
    direction: Option<String>,
    runtime_rtl_letters: Option<&HashSet<char>>,
) -> Result<String, String> {
    if direction.as_deref() == Some("rtl") {
        anchor(text, kind.as_deref().unwrap_or(""), runtime_rtl_letters)
    } else {
        Ok(text)
    }
}

pub(crate) fn wrap_subtitle_lines_with_ass_context(
    text: String,
    max_chars: i64,
    max_lines: i64,
    context: crate::markup::bidi_prose::AssContext,
) -> Result<String, String> {
    lines::wrap_with_context(text, max_chars, max_lines, "ass", context)
}

pub(crate) fn anchor_rtl_line_with_ass_context(
    text: String,
    kind: &str,
    direction: &str,
    context: crate::markup::bidi_prose::AssContext,
) -> Result<String, String> {
    if direction == "rtl" {
        lines::anchor_with_context(text, kind, None, context)
    } else {
        Ok(text)
    }
}

pub fn process_cue_text(
    text: String,
    kind: Option<String>,
    direction: Option<String>,
    max_chars: Option<i64>,
    fix_zwnj: Option<bool>,
    wrap_lines: Option<bool>,
) -> Result<String, String> {
    let text = prepare_cue_text(
        text,
        kind.clone(),
        direction.clone(),
        max_chars,
        fix_zwnj,
        wrap_lines,
    )?;
    anchor_rtl_line(text, kind, direction)
}

pub fn prepare_cue_text(
    text: String,
    kind: Option<String>,
    direction: Option<String>,
    max_chars: Option<i64>,
    fix_zwnj: Option<bool>,
    wrap_lines: Option<bool>,
) -> Result<String, String> {
    prepare_cue_text_with_word_class(text, kind, direction, max_chars, fix_zwnj, wrap_lines, None)
}

pub fn prepare_cue_text_with_word_class(
    mut text: String,
    kind: Option<String>,
    direction: Option<String>,
    max_chars: Option<i64>,
    fix_zwnj: Option<bool>,
    wrap_lines: Option<bool>,
    runtime_word_class: Option<&str>,
) -> Result<String, String> {
    let kind_str = kind.as_deref().unwrap_or("");
    let is_rtl = direction.as_deref() == Some("rtl");
    if kind_str == "ass" {
        text = convert_srt_markup_to_ass(text)?;
    }
    if is_rtl && fix_zwnj.unwrap_or(false) {
        text = normalize_persian_zwnj_with_word_class(text, runtime_word_class)?;
    }
    if wrap_lines.unwrap_or(false) {
        let max_chars = max_chars.ok_or_else(|| {
            "TypeError: '<' not supported between instances of 'NoneType' and 'int'".to_string()
        })?;
        text = wrap(text, max_chars, 2, kind_str)?;
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_ass_html_participates_in_wrapping_and_bidi() {
        let source = "<font color=red>one two three</font> four five";
        let wrapped = wrap_subtitle_lines(source.into(), Some(24), Some(2)).unwrap();
        assert!(wrapped.contains(r"\N"));
        assert_eq!(wrapped.replace(r"\N", " "), source);
        let literal = "<b title=\"سلام\">.</b>";
        let anchored =
            anchor_rtl_line(literal.into(), Some("ass".into()), Some("rtl".into())).unwrap();
        assert!(anchored.starts_with("\u{200f}\u{2067}"));
        assert!(anchored.contains(literal));
    }

    #[test]
    fn watermark_markers_inside_dialogue_are_preserved() {
        for text in [
            "Open https://example.test to read the manual.",
            "They used OpenSubtitles for the experiment.",
            "Send that address, t.me/channel, to your friend.",
            "Deluxe Media is mentioned in the documentary.",
            "I asked who the subtitles by Alice were for.",
            "Downloaded from the device, the files contain evidence.",
            "Subtitles by amateurs can contain mistakes.",
            "Subtitles by amateurs are unreliable.",
            "«SUBTITLES BY AMATEURS ARE UNRELIABLE!»",
            "Subtitles by humans fail.",
            "Captioned by humans looks better.",
            "Encoded by machines looks better.",
            "Subtitles by amateurs\nare unreliable.",
            "Subtitles by amateurs<br>are unreliable.",
            "Subtitles by John O'Connor",
            "Downloaded from the web",
            "زیرنویس از دو دوست",
            "<notice>https://example.test</notice>",
            "https://example.test</notice>",
            "OpenSubtitles.org/path<notice>",
            "t.me/channel{literal}",
            r"{\pos(1,2)}https://example.test",
            "من این کانال تلگرام را نمی‌شناسم.",
            "Save the evidence.\nhttps://example.test",
            "Save the evidence.<br>https://example.test",
            "Save the evidence.\\N@channel",
        ] {
            assert_eq!(watermark_reason(text.into()).as_deref(), Ok(""), "{text}");
        }
        assert_eq!(
            watermark_reason("OpenSubtitles.org".into()).as_deref(),
            Ok("site-watermark")
        );
        assert_eq!(
            watermark_reason("<i>Subtitles by Alice</i><br>https://example.test".into()).as_deref(),
            Ok("fansub-credit")
        );
        for text in [
            "Subtitles by @actual_team",
            "Downloaded from https://example.test",
        ] {
            assert_eq!(
                watermark_reason(text.into()).as_deref(),
                Ok("fansub-credit")
            );
        }
    }

    #[test]
    fn watermark_visibility_respects_format_and_preserves_literal_syntax() {
        for (kind, text, expected) in [
            ("srt", "<i>https://example.test</i>", "url-ad"),
            ("ass", "<i>https://example.test</i>", ""),
            ("srt", "<notice>https://example.test</notice>", ""),
            ("srt", r"{\pos(1,2)}https://example.test", ""),
            ("srt", "{an8}https://example.test", "url-ad"),
            ("srt", r"{\an8}https://example.test", "url-ad"),
            ("ass", r"{\i1}https://example.test{\i0}", "url-ad"),
            ("ass", r"\{literal\}https://example.test", ""),
            ("ass", r"{\p1}m 0 0 l 1 1{\p0}https://example.test", ""),
            ("srt", "<!--hidden-->https://example.test", "url-ad"),
            ("ass", "<!--literal-->https://example.test", ""),
        ] {
            assert_eq!(
                watermark_reason_with_kind(text.into(), kind).as_deref(),
                Ok(expected)
            );
            assert_eq!(
                is_watermark_cue_with_kind(text.into(), kind),
                Ok(!expected.is_empty())
            );
        }
        assert!(watermark_reason_with_kind("text".into(), "vtt").is_err());
    }

    #[test]
    fn persian_joining_leaves_markup_and_urls_alone() {
        assert_eq!(
            normalize_persian_zwnj("می رود <i>حلقه ها</i> https://x.test/مي".into()).as_deref(),
            Ok("می\u{200c}رود <i>حلقه\u{200c}ها</i> https://x.test/مي")
        );
    }

    #[test]
    fn watermark_prioritizes_studio_credit_and_respects_boundaries() {
        assert_eq!(
            watermark_reason("<b>DELUXE MEDIA</b><br>Sync and corrected by".into()).as_deref(),
            Ok("studio-credit")
        );
        assert_eq!(
            is_watermark_cue("says opensubtitlesX in dialogue".into()),
            Ok(false)
        );
        assert_eq!(is_watermark_cue("<b>ｘ</b> t.me/subs".into()), Ok(false));
        assert_eq!(is_watermark_cue("t.me/subs".into()), Ok(true));
    }

    #[test]
    fn markup_conversion_restores_outer_color() {
        assert_eq!(
            convert_srt_markup_to_ass("{an8}<i>A</i><font color=#123456>x<font color=\"#AABBCC\">y</font>z</font><br>q\r\nR".into()).as_deref(),
            Ok(r"{\an8}{\i1}A{\i0}{\c&H563412&}x{\c&HCCBBAA&}y{\c&H563412&}z{\c}\Nq\NR")
        );
    }

    #[test]
    fn wrapping_balances_words_and_rejects_nonpositive_width() {
        assert_eq!(
            wrap_subtitle_lines("one two three four five six".into(), Some(13), Some(2)).as_deref(),
            Ok(r"one two three\Nfour five six")
        );
        assert_eq!(
            wrap_subtitle_lines("hello".into(), Some(0), Some(2))
                .as_deref()
                .map_err(String::as_str),
            Err("ValueError: Line wrapping requires positive max_chars and max_lines")
        );
    }

    #[test]
    fn drawing_mode_errors_match_python() {
        assert_eq!(
            wrap_subtitle_lines(r"{\t(0,1,\p(-1))}سلام".into(), Some(42), Some(2))
                .as_deref()
                .map_err(String::as_str),
            Err("ValueError: ASS drawing mode must be a nonnegative integer")
        );
        assert_eq!(
            anchor_rtl_line(
                r"{\pos(1,2}سلام".into(),
                Some("ass".into()),
                Some("rtl".into())
            )
            .as_deref()
            .map_err(String::as_str),
            Err("ValueError: Unbalanced ASS override parentheses")
        );
    }

    #[test]
    fn rtl_anchor_keeps_overrides_and_is_idempotent() {
        let anchored = "{\\i1}\u{200f}\u{2067}- سلام!\u{2069}{\\i0}";
        for text in [r"{\i1}- سلام!{\i0}", anchored] {
            assert_eq!(
                anchor_rtl_line(text.into(), Some("ass".into()), Some("rtl".into())).as_deref(),
                Ok(anchored)
            );
        }
    }

    #[test]
    fn cue_processing_converts_before_anchoring_and_normalizing() {
        assert_eq!(
            process_cue_text(
                "<i>می رود!</i><br>سلام؟".into(),
                Some("ass".into()),
                Some("rtl".into()),
                Some(42),
                Some(true),
                Some(true)
            )
            .as_deref(),
            Ok("{\\i1}\u{200f}\u{2067}می\u{200c}رود!\u{2069}{\\i0}\\N\u{200f}\u{2067}سلام؟\u{2069}")
        );
    }
}
