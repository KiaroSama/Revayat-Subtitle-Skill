use regex::Regex;
use std::sync::LazyLock;

static BREAK: LazyLock<Result<Regex, regex::Error>> =
    LazyLock::new(|| Regex::new(r"(?i)\A</?br/?(?: [^<>]*)?>\z"));
static ALIGN: LazyLock<Result<Regex, regex::Error>> =
    LazyLock::new(|| Regex::new(r"(?i-u)\A\{\\?an[1-9]\}\z"));
static DONOR: LazyLock<Result<Regex, regex::Error>> =
    LazyLock::new(|| Regex::new(r"<([^<>]{0,128})>"));
static SIMPLE: LazyLock<Result<Regex, regex::Error>> =
    LazyLock::new(|| Regex::new(r"(?i-u)\A(?:</?[bisu]>|</font>)\z"));
static COLOR: LazyLock<Result<Regex, regex::Error>> = LazyLock::new(|| {
    Regex::new(r##"(?i-u)\A<font +color *= *(?:"#[0-9a-f]{6}"|'#[0-9a-f]{6}'|#[0-9a-f]{6}) *>\z"##)
});
fn re(re: &LazyLock<Result<Regex, regex::Error>>) -> Result<&Regex, String> {
    re.as_ref()
        .map_err(|_| super::invalid("Invalid subtitle markup pattern"))
}
pub(super) use crate::markup::subtitle_pieces as pieces;

pub(super) fn remove_comments(s: String) -> Result<String, String> {
    crate::markup::uncomment(s, "srt".into())
}
pub(super) fn alignment(s: &str) -> Result<bool, String> {
    Ok(re(&ALIGN)?.is_match(s))
}
pub(super) fn validate_donor(text: &str) -> Result<(), String> {
    for cap in re(&DONOR)?.captures_iter(text) {
        let (Some(m), Some(body)) = (cap.get(0), cap.get(1)) else {
            continue;
        };
        if !crate::markup::srt_tag_fits(m.as_str().into())? {
            continue;
        }
        let body = body.as_str();
        let closing = body.starts_with('/');
        let body = body.strip_prefix('/').unwrap_or(body);
        let name = body.trim_start_matches(' ').split(' ').next().unwrap_or("");
        let recognized = matches!(
            name.to_ascii_lowercase().as_str(),
            "b" | "i" | "s" | "u" | "font" | "br" | "br/"
        ) || (!closing
            && body
                .get(..4)
                .is_some_and(|s| s.eq_ignore_ascii_case("font"))
            && body.get(4..).is_some_and(|suffix| {
                suffix.starts_with(|ch: char| ch.is_ascii_whitespace())
                    && !suffix.trim_ascii().is_empty()
            }));
        let likely = name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_/".contains(&b))
            && !body.starts_with(' ')
            && (closing || m.start() == 0 || text.as_bytes()[m.start() - 1] != b'<');
        if (recognized || likely)
            && !re(&BREAK)?.is_match(m.as_str())
            && !re(&COLOR)?.is_match(m.as_str())
            && !re(&SIMPLE)?.is_match(m.as_str())
        {
            return Err(super::invalid(
                "SRT donor uses unsupported decoder markup; adapt it explicitly before ASS conversion",
            ));
        }
    }
    Ok(())
}
pub(super) fn ass_commands(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for (kind, tag) in pieces(text, "ass")? {
        if kind == "tag" {
            out.extend(crate::markup::ass_commands(tag)?);
        }
    }
    Ok(out)
}

pub(super) fn contains_ass_control(text: &str) -> Result<bool, String> {
    let mut after = 0;
    for (i, c) in text.match_indices(['{', '}', '\\']) {
        if i < after {
            continue;
        }
        if c == "{" && (i == 0 || text.as_bytes()[i - 1] != b'\\') {
            let rest = &text[i..];
            for length in [5, 6] {
                if let Some(candidate) = rest.get(..length)
                    && alignment(candidate)?
                {
                    after = i + length;
                    break;
                }
            }
        }
        if i >= after {
            return Ok(true);
        }
    }
    Ok(false)
}
