//! Native implementation of the Python subtitle text pipeline.
#![forbid(unsafe_code)]

use fancy_regex::Regex as FancyRegex;
use regex::Regex;

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

mod drawing;
mod lines;
mod prose;

use lines::{anchor, wrap};

const ZWNJ: char = '\u{200c}';
const RLM: char = '\u{200f}';
const RLE: char = '\u{202b}';
const PDF: char = '\u{202c}';
const BIDI: &str = "\u{061c}\u{200e}\u{200f}\u{202a}\u{202b}\u{202c}\u{202d}\u{202e}\u{2066}\u{2067}\u{2068}\u{2069}";
const ASS: &str = r"(?<!\\)\{[^{}]*\}";
const HTML: &str = r"<!--(?s:.*?)-->|</?[A-Za-z][^<>]*>";
const WORD: &str = r"\p{L}\p{N}_";

fn regex_error(error: fancy_regex::Error) -> String {
    format!("RuntimeError: subtitle regex failed: {error}")
}

fn tags() -> &'static FancyRegex {
    pattern!(FancyRegex, format!("{ASS}|{HTML}"))
}

fn alignment() -> &'static Regex {
    pattern!(Regex, r"\A\{\\?(?i-u:an)[1-9]\}\z")
}

fn srt_break(tag: &str) -> bool {
    pattern!(Regex, r"^</?(?i-u:br)/?(?: [^<>]*)?>\z").is_match(tag)
        && tag.chars().count() <= if tag.starts_with("</") { 130 } else { 129 }
}

fn srt_tag_fits(tag: &str) -> bool {
    let delimiters = if tag.starts_with("</") { 3 } else { 2 };
    tag.len().saturating_sub(delimiters) <= 127
}

fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

fn segments<'a>(
    text: &'a str,
    engine: &'static FancyRegex,
) -> impl Iterator<Item = Result<(&'a str, &'a str), String>> {
    let mut end = 0;
    engine
        .find_iter(text)
        .map(Some)
        .chain(std::iter::once(None))
        .map(move |m| {
            if let Some(m) = m {
                let m = m.map_err(regex_error)?;
                let prose = &text[end..m.start()];
                end = m.end();
                Ok((prose, m.as_str()))
            } else {
                Ok((&text[end..], ""))
            }
        })
}

pub fn normalize_persian_zwnj(text: String) -> Result<String, String> {
    prose::normalize(&text)
}

pub fn watermark_reason(text: String) -> Result<String, String> {
    prose::watermark(&text).map(str::to_owned)
}

pub fn is_watermark_cue(text: String) -> Result<bool, String> {
    Ok(!prose::watermark(&text)?.is_empty())
}

pub fn convert_srt_markup_to_ass(text: String) -> Result<String, String> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut output = String::with_capacity(text.len());
    let mut colors = Vec::new();
    let color = pattern!(
        Regex,
        r##"\A(?i-u:<font) +(?i-u:color) *= *(?:"#([0-9A-Fa-f]{6})"|'#([0-9A-Fa-f]{6})'|#([0-9A-Fa-f]{6})) *>\z"##
    );
    let simple = pattern!(Regex, r"\A(?i-u:</?[bisu]>|</font>)\z");
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
    if direction.as_deref() == Some("rtl") {
        anchor(text, kind.as_deref().unwrap_or(""))
    } else {
        Ok(text)
    }
}

pub fn process_cue_text(
    mut text: String,
    kind: Option<String>,
    direction: Option<String>,
    max_chars: Option<i64>,
    fix_zwnj: Option<bool>,
    wrap_lines: Option<bool>,
) -> Result<String, String> {
    let kind_str = kind.as_deref().unwrap_or("");
    let is_rtl = direction.as_deref() == Some("rtl");
    if kind_str == "ass" {
        text = convert_srt_markup_to_ass(text)?;
    }
    if is_rtl && fix_zwnj.unwrap_or(false) {
        text = normalize_persian_zwnj(text)?;
    }
    if wrap_lines.unwrap_or(false) {
        let max_chars = max_chars.ok_or_else(|| {
            "TypeError: '<' not supported between instances of 'NoneType' and 'int'".to_string()
        })?;
        text = wrap(text, max_chars, 2, kind_str)?;
    }
    anchor_rtl_line(text, kind, direction)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(is_watermark_cue("<b>ｘ</b> t.me/subs".into()), Ok(true));
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
        let anchored = "{\\i1}\u{202b}\u{200f}- سلام!\u{200f}\u{202c}{\\i0}";
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
            Ok("{\\i1}\u{202b}می\u{200c}رود!\u{200f}\u{202c}{\\i0}\\N\u{202b}سلام؟\u{200f}\u{202c}")
        );
    }
}
