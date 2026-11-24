//! Format-aware text spans shared by wrapping and bidi validation.

use super::drawing::drawing_mode;
use super::{Regex, srt_tag_fits};

pub(super) fn segments<'a>(text: &'a str, kind: &str) -> Result<Vec<(&'a str, &'a str)>, String> {
    let (mut cursor, mut search, mut drawing) = (0, 0, false);
    let mut no_comment_end = false;
    let mut output = Vec::new();
    let srt_blocks = pattern!(
        Regex,
        r"</?(?i-u:br)/?(?: [^<>]*)?>|<!--|</?(?i-u:i|b|u|s|font)(?: [^<>]*)?>|\{\\(?i-u:an)[1-9]\}"
    );
    while search < text.len() {
        let (start, end) = if kind == "ass" {
            let Some(start) = text[search..].find('{').map(|i| search + i) else {
                break;
            };
            search = start + 1;
            if !drawing && start > 0 && text.as_bytes()[start - 1] == b'\\' {
                continue;
            }
            let Some(end) = text[search..].find('}').map(|i| search + i + 1) else {
                break;
            };
            drawing = drawing_mode(&text[start..end], drawing)?;
            (start, end)
        } else {
            let Some(m) = srt_blocks.find_at(text, search) else {
                break;
            };
            search = m.end();
            if m.as_str() == "<!--" {
                if !no_comment_end && let Some(end) = text[search..].find("-->") {
                    search += end + 3;
                } else {
                    no_comment_end = true;
                    continue;
                }
            } else if !srt_tag_fits(m.as_str()) {
                continue;
            }
            (m.start(), search)
        };
        output.push((&text[cursor..start], &text[start..end]));
        cursor = end;
        search = end;
    }
    output.push((&text[cursor..], ""));
    Ok(output)
}
