use super::{FancyRegex, Regex, regex_error, tag_fits};
use std::ops::Range;

pub(super) fn overrides(
    block: &str,
    mut visit: impl FnMut(&str, Range<usize>) -> Result<(), String>,
) -> Result<(), String> {
    if let Some(body) = block.strip_prefix('{').and_then(|s| s.strip_suffix('}'))
        && !body.contains('}')
    {
        scan(block, 1..block.len() - 1, 0, &mut visit)?;
    }
    Ok(())
}

fn scan(
    block: &str,
    range: Range<usize>,
    depth: usize,
    visit: &mut impl FnMut(&str, Range<usize>) -> Result<(), String>,
) -> Result<(), String> {
    if depth > 16 {
        return Err("ValueError: ASS transform nesting exceeds 16 levels".into());
    }
    let mut cursor = range.start;
    let mut count = 0;
    let trim = [' ', '\t'];
    let names = pattern!(
        Regex,
        r"\A(?:fscx|fscy|fsc|iclip|alpha|xbord|ybord|xshad|yshad|border|blur|bord|shad|move|fade|clip|frx|fry|frz|fr|be|fax|fay|pbo|pos|org|fad|fsp|fn|fs|fe|kf|ko|kt|an|[1-4][ac]|[biuskKqrptac])"
    );
    while let Some(slash) = block[cursor..range.end].find('\\') {
        let after = cursor + slash + 1;
        let tail = block[after..range.end].trim_start_matches(trim);
        let Some(name) = names.find(tail) else {
            cursor = after;
            continue;
        };
        let name = name.as_str();
        let begin = range.end - tail.len() + name.len();
        let arg = block[begin..range.end]
            .split('\\')
            .next()
            .unwrap_or_default();
        if let Some(open) = arg.find('(') {
            let start = begin + open + 1;
            let complex = matches!(
                name,
                "t" | "clip" | "iclip" | "pos" | "org" | "move" | "fad" | "fade"
            );
            let mut level = 1usize;
            let closing = block.as_bytes()[start..range.end]
                .iter()
                .position(|&c| {
                    level += usize::from(complex && c == b'(');
                    level -= usize::from(c == b')');
                    level == 0
                })
                .ok_or("ValueError: Unbalanced ASS override parentheses")?
                + start;
            cursor = closing + 1;
            if complex {
                visit(name, start - 1..cursor)?;
                if name == "t"
                    && let Some((prefix, _)) = block[start..closing].split_once('\\')
                    && prefix.rsplit_once(',').is_none_or(|(args, _)| {
                        args.split(',')
                            .filter(|arg| !arg.trim_matches(trim).is_empty())
                            .take(4)
                            .count()
                            <= 3
                    })
                {
                    scan(block, start + prefix.len()..closing, depth + 1, visit)?;
                }
            } else {
                let mut offset = start;
                let mut scalar = begin..begin + arg[..open].trim_end_matches(trim).len();
                for part in block[start..closing].split(',') {
                    let leading = part.trim_start_matches(trim);
                    let size = leading.trim_end_matches(trim).len();
                    if size != 0 {
                        let from = offset + part.len() - leading.len();
                        scalar = from..from + size;
                        break;
                    }
                    offset += part.len() + 1;
                }
                visit(name, scalar)?;
            }
        } else {
            cursor = begin + arg.len();
            visit(name, begin..begin + arg.trim_end_matches(trim).len())?;
        }
        count += 1;
        if count > 4096 {
            return Err("ValueError: ASS override block exceeds token budget".into());
        }
    }
    Ok(())
}

pub(crate) fn drawing_in(block: &str, mut drawing: bool) -> Result<bool, String> {
    overrides(block, |name, range| {
        if name == "p" {
            let arg = block[range].trim_matches([' ', '\t']);
            let digits = arg.strip_prefix('+').unwrap_or(arg);
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return Err("ValueError: ASS drawing mode must be a nonnegative integer".into());
            }
            drawing = digits.bytes().any(|b| b != b'0');
        }
        Ok(())
    })?;
    Ok(drawing)
}

const SRT_BREAK: &str = r"</?(?=[^<>]{0,127}>)[Bb][Rr]/?(?: [^<>]*)?>";

fn srt_break(block: &str) -> Result<bool, String> {
    pattern!(FancyRegex, &format!(r"\A{SRT_BREAK}\z"))
        .is_match(block)
        .map_err(regex_error)
}

pub(super) fn block_spans<'a>(
    text: &'a str,
    kind: &'a str,
) -> impl Iterator<Item = Result<(Range<usize>, bool), String>> + 'a {
    let (mut cursor, mut no_comment_end, mut drawing) = (0, false, false);
    std::iter::from_fn(move || {
        while cursor < text.len() {
            if kind == "ass" {
                let start = cursor + text[cursor..].find('{')?;
                if !drawing && start > 0 && text.as_bytes()[start - 1] == b'\\' {
                    cursor = start + 1;
                    continue;
                }
                let end = start + 2 + text[start + 1..].find('}')?;
                cursor = end;
                return Some(drawing_in(&text[start..end], drawing).map(|next| {
                    drawing = next;
                    (start..end, next)
                }));
            }
            let srt = pattern!(
                FancyRegex,
                &format!(
                    r"{SRT_BREAK}|<!--|</?(?:[Ii]|[Bb]|[Uu]|[Ss]|[Ff][Oo][Nn][Tt])(?: [^<>]*)?>|\{{\\[Aa][Nn][1-9]\}}"
                )
            );
            let token = match srt.find_from_pos(text, cursor) {
                Ok(token) => token?,
                Err(error) => return Some(Err(regex_error(error))),
            };
            let start = token.start();
            cursor = token.end();
            if token.as_str() == "<!--" {
                if no_comment_end {
                    continue;
                }
                let Some(end) = text[cursor..].find("-->") else {
                    no_comment_end = true;
                    continue;
                };
                cursor += end + 3;
            } else if !tag_fits(token.as_str()) {
                continue;
            }
            return Some(Ok((start..cursor, false)));
        }
        None
    })
}

pub(super) fn hidden(block: &str, kind: &str) -> bool {
    match kind {
        "ass" => !block.contains('\\'),
        "srt" => block.starts_with("<!--"),
        _ => false,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Piece {
    Text,
    Tag,
    Drawing,
}
use Piece::{Drawing, Tag, Text};

pub(super) fn pieces<'a>(
    text: &'a str,
    kind: &str,
    mut visit: impl FnMut(Piece, &'a str) -> Result<(), String>,
) -> Result<(), String> {
    let (mut cursor, mut drawing) = (0, false);
    for span in block_spans(text, kind) {
        let (range, next) = span?;
        if range.start > cursor {
            visit(
                if drawing { Drawing } else { Text },
                &text[cursor..range.start],
            )?;
        }
        cursor = range.end;
        let block = &text[range];
        if hidden(block, kind) {
            if drawing {
                visit(Tag, "{}")?;
            }
        } else if kind == "srt" && srt_break(block)? {
            visit(Text, "\n")?;
        } else {
            visit(Tag, block)?;
        }
        drawing = next;
    }
    if cursor < text.len() {
        visit(if drawing { Drawing } else { Text }, &text[cursor..])?;
    }
    Ok(())
}
