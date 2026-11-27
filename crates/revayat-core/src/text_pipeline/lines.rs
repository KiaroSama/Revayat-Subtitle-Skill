use crate::markup::bidi::{PDI, RTL_ISOLATE, has_outer_rtl_isolate};
use std::collections::HashSet;
use unicode_bidi::{BidiClass, bidi_class};

use super::drawing_mode;
use super::tokens;
use super::{BIDI, PDF, RLE, Regex, WORD, ZWNJ, py_space, srt_break, srt_tag_fits};

const WRAP_PUNCT: &str = "،.؟?!؛:…";
const EDGE_MARKS: &str = "\u{200f}\u{200e}\u{061c}";

fn trim_edge_marks(head: &mut String, leading: bool) {
    if leading {
        let end = head.len() - head.trim_start_matches(|c| EDGE_MARKS.contains(c)).len();
        head.drain(..end);
    } else {
        let end = head.trim_end_matches(py_space).len();
        let start = head[..end]
            .trim_end_matches(|c| EDGE_MARKS.contains(c))
            .len();
        head.drain(start..end);
    }
}

fn repair_leading_punctuation(parts: &mut [String], first: usize, last: usize) {
    let start = parts[first].trim_start_matches(|c| py_space(c) || BIDI.contains(c));
    let prefix = [
        "...", "?!", "!?", "؟!", "!؟", ".", "،", "؛", "!", "؟", "?", "…",
    ]
    .into_iter()
    .find(|prefix| {
        start
            .strip_prefix(prefix)
            .is_some_and(|body| body.chars().next().is_some_and(super::prose::arabic_letter))
    });
    let Some(prefix) = prefix else { return };
    let tail = parts[last].trim_end_matches(|c| py_space(c) || BIDI.contains(c));
    if tail.ends_with(|c| WRAP_PUNCT.contains(c)) {
        return;
    }
    let offset = parts[first].len() - start.len();
    let end = tail.len() - if first == last { prefix.len() } else { 0 };
    parts[first].drain(offset..offset + prefix.len());
    parts[last].insert_str(end, prefix);
}

type Token<'a> = (&'a str, bool);
type Line<'a> = Vec<Token<'a>>;

fn lines<'a>(text: &'a str, kind: &str) -> Result<(Vec<Line<'a>>, Vec<&'a str>), String> {
    let mut lines = Vec::new();
    let mut line = Vec::new();
    let mut breaks = Vec::new();
    let mut drawing = false;
    let newlines = if kind == "ass" {
        pattern!(Regex, r"\\N|\r\n|\n|\r")
    } else {
        pattern!(Regex, r"\r\n|\n|\r")
    };
    for (prose, tag) in tokens::segments(text, kind)? {
        let mut start = 0;
        for m in newlines.find_iter(prose) {
            if m.start() > start {
                line.push((&prose[start..m.start()], !drawing));
            }
            lines.push(std::mem::take(&mut line));
            breaks.push(m.as_str());
            start = m.end();
        }
        if start < prose.len() {
            line.push((&prose[start..], !drawing));
        }
        if tag.is_empty() {
            continue;
        }
        if kind == "ass" && tag.starts_with('{') {
            drawing = drawing_mode(tag, drawing)?;
        }
        if kind == "srt" && srt_break(tag) && srt_tag_fits(tag) {
            breaks.push(tag);
            lines.push(std::mem::take(&mut line));
        } else {
            line.push((tag, false));
        }
    }
    lines.push(line);
    Ok((lines, breaks))
}

fn visible(line: &[Token<'_>], kind: &str) -> String {
    let text: String = line
        .iter()
        .filter(|(_, show)| *show)
        .flat_map(|(part, _)| part.chars())
        .filter(|&c| c != ZWNJ && !BIDI.contains(c))
        .collect();
    if kind == "ass" {
        text.replace(r"\n", " ")
            .replace(r"\h", " ")
            .replace(r"\{", "{")
            .replace(r"\}", "}")
    } else {
        text
    }
}

fn original(line: &[Token<'_>]) -> String {
    line.iter().map(|(part, _)| *part).collect()
}

fn wrap_group(lines: &[Line<'_>], width: usize, limit: usize, kind: &str) -> String {
    let mut merged = Vec::new();
    for line in lines {
        if !line.is_empty() && !merged.is_empty() {
            merged.push((" ", true));
        }
        merged.extend_from_slice(line);
    }
    let raw = original(&merged);
    let visible_chars: Vec<_> = visible(&merged, kind).chars().collect();
    let total = visible_chars.len();
    if total <= width || limit <= 1 {
        return raw;
    }
    let spaces = pattern!(Regex, r" +");
    let boundary = pattern!(Regex, format!(r"(?:،|(?:\A|[^{WORD}])(?:و|که|اما|ولی))\z"));
    let (mut offset, mut before, mut tail) = (0, 0, String::new());
    let mut candidates = Vec::new();
    let extend = |chunk: &str, tail: &mut String| {
        let chunk = visible(&[(chunk, true)], kind);
        tail.push_str(&chunk);
        if let Some((start, _)) = tail.char_indices().rev().nth(4) {
            tail.drain(..start);
        }
        chunk.chars().count()
    };
    for (part, show) in merged {
        if show {
            let mut cursor = 0;
            for space in spaces.find_iter(part) {
                before += extend(&part[cursor..space.start()], &mut tail);
                let space_len = space.len();
                if before > 0
                    && before < total.saturating_sub(space_len)
                    && !visible_chars
                        .get(before + space_len)
                        .is_some_and(|c| WRAP_PUNCT.contains(*c))
                {
                    candidates.push((
                        offset + space.start(),
                        offset + space.end(),
                        before,
                        boundary.is_match(&tail),
                    ));
                }
                before += space_len;
                tail.clear();
                tail.push(' ');
                cursor = space.end();
            }
            before += extend(&part[cursor..], &mut tail);
        }
        offset += part.len();
    }
    let mut count = limit.min((total / (width + 1) + 1).max(2));
    let (mut start, mut consumed) = (0, 0);
    let mut output = String::new();
    let delimiter = if kind == "ass" { r"\N" } else { "\n" };
    while count > 1 {
        let remaining = (total as i128) - (consumed as i128);
        let target = (remaining - 1) as f64 / count as f64;
        let score = |(left, right, at, pause): (usize, usize, usize, bool)| {
            let size = (at as i128) - (consumed as i128);
            let overflow = (size - width as i128)
                .max(
                    remaining
                        - size
                        - (right - left) as i128
                        - (width as i128) * ((count - 1) as i128)
                        - (count - 2) as i128,
                )
                .max(0);
            (
                overflow,
                (size as f64 - target).abs() - if pause { 2.0 } else { 0.0 },
                left,
            )
        };
        let Some(&(left, right, at, _)) = candidates.iter().min_by(|&&a, &&b| {
            let (overflow_a, delta_a, left_a) = score(a);
            let (overflow_b, delta_b, left_b) = score(b);
            overflow_a
                .cmp(&overflow_b)
                .then_with(|| delta_a.total_cmp(&delta_b))
                .then(left_a.cmp(&left_b))
        }) else {
            break;
        };
        output.push_str(&raw[start..left]);
        output.push_str(delimiter);
        start = right;
        consumed = at + right - left;
        candidates.retain(|candidate| candidate.0 > right);
        count -= 1;
    }
    output.push_str(&raw[start..]);
    output
}

pub(super) fn wrap(text: String, width: i64, limit: i64, kind: &str) -> Result<String, String> {
    if width < 1 || limit < 1 {
        return Err("ValueError: Line wrapping requires positive max_chars and max_lines".into());
    }
    let width = usize::try_from(width).unwrap_or(usize::MAX);
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    if kind == "ass" {
        let mut drawing = false;
        for (prose, tag) in tokens::segments(&text, kind)? {
            if drawing && !prose.is_empty() {
                return Ok(text);
            }
            drawing = drawing_mode(tag, drawing)?;
        }
    }
    let (lines, breaks) = lines(&text, kind)?;
    let visible: Vec<_> = lines
        .iter()
        .map(|line| visible(line, kind).trim_matches(py_space).to_owned())
        .collect();
    let lengths: Vec<_> = visible.iter().map(|line| line.chars().count()).collect();
    if lengths.iter().all(|&len| len <= width) {
        return Ok(text);
    }
    let starts: Vec<_> = std::iter::once(0)
        .chain(visible.iter().enumerate().skip(1).filter_map(|(i, line)| {
            (line.starts_with("- ") || line.starts_with("– ")).then_some(i)
        }))
        .chain(std::iter::once(lines.len()))
        .collect();
    let group_limit = limit.saturating_sub(starts.len() - 2).max(1);
    let mut output = String::with_capacity(text.len());
    for start in starts.windows(2) {
        let (begin, end) = (start[0], start[1]);
        if begin > 0 {
            output.push_str(breaks[begin - 1]);
        }
        if lengths[begin..end].iter().all(|&len| len <= width) {
            for i in begin..end {
                output.push_str(&original(&lines[i]));
                if i + 1 < end {
                    output.push_str(breaks[i]);
                }
            }
        } else {
            output.push_str(&wrap_group(&lines[begin..end], width, group_limit, kind));
        }
    }
    Ok(output)
}

fn balanced(text: &str, kind: &str) -> bool {
    let mut stack = Vec::new();
    let mut feed = |prose: &str| {
        for c in prose.chars() {
            match c {
                '\u{202a}' | '\u{202b}' | '\u{202d}' | '\u{202e}' => stack.push(false),
                '\u{2066}'..='\u{2068}' => stack.push(true),
                PDF if stack.pop() != Some(false) => return false,
                '\u{2069}' => {
                    while stack.last() == Some(&false) {
                        stack.pop();
                    }
                    if stack.pop().is_none() {
                        return false;
                    }
                }
                _ => {}
            }
            if stack.len() > 125 {
                return false;
            }
        }
        true
    };
    let Ok(tokens) = tokens::segments(text, kind) else {
        return false;
    };
    let mut drawing = false;
    for (prose, tag) in tokens {
        if !drawing && !feed(prose) {
            return false;
        }
        if kind == "ass" {
            let Ok(next) = drawing_mode(tag, drawing) else {
                return false;
            };
            drawing = next;
        }
    }
    stack.is_empty()
}

pub(super) fn anchor(
    text: String,
    kind: &str,
    runtime_rtl_letters: Option<&HashSet<char>>,
) -> Result<String, String> {
    let (lines, breaks) = lines(&text, kind)?;
    let whole_balanced = balanced(&text, kind);
    let alpha = pattern!(Regex, r"\A\p{L}\z");
    let mut output = String::with_capacity(text.len());
    for (i, line) in lines.iter().enumerate() {
        let mut raw = original(line);
        let prose = visible(line, kind);
        let has_rtl = prose.chars().any(|c| {
            if let Some(letters) = runtime_rtl_letters {
                return letters.contains(&c);
            }
            matches!(bidi_class(c), BidiClass::R | BidiClass::AL)
                && alpha.is_match(c.encode_utf8(&mut [0; 4]))
        });
        let spans = line
            .iter()
            .filter(|(part, show)| *show && !part.trim_matches(py_space).is_empty());
        let already_isolated = has_outer_rtl_isolate(spans.flat_map(|(part, _)| part.chars()));
        if has_rtl {
            let line_balanced = balanced(&raw, kind);
            if already_isolated && line_balanced {
                output.push_str(&raw);
                if let Some(delimiter) = breaks.get(i) {
                    output.push_str(delimiter);
                }
                continue;
            }
            if line_balanced || !whole_balanced {
                let mut indices = line
                    .iter()
                    .enumerate()
                    .filter(|(_, token)| token.1 && !token.0.trim_matches(py_space).is_empty());
                if let Some((first, _)) = indices.next() {
                    let last = indices.next_back().map_or(first, |(last, _)| last);
                    let mut parts: Vec<_> =
                        line.iter().map(|(part, _)| (*part).to_owned()).collect();
                    if !line_balanced {
                        for (part, (_, show)) in parts.iter_mut().zip(line) {
                            if *show {
                                part.retain(|c| !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'));
                            }
                        }
                    }
                    trim_edge_marks(&mut parts[first], true);
                    trim_edge_marks(&mut parts[last], false);
                    if line_balanced
                        && parts[first].starts_with(RLE)
                        && parts[last].trim_end_matches(py_space).ends_with(PDF)
                    {
                        let (a, b) = (parts[first].clone(), parts[last].clone());
                        parts[first].drain(..RLE.len_utf8());
                        let end = parts[last].trim_end_matches(py_space).len();
                        parts[last].drain(end - PDF.len_utf8()..end);
                        if balanced(&parts.concat(), kind) {
                            trim_edge_marks(&mut parts[first], true);
                            trim_edge_marks(&mut parts[last], false);
                        } else {
                            parts[first] = a;
                            parts[last] = b;
                        }
                    }
                    repair_leading_punctuation(&mut parts, first, last);
                    parts[first].insert_str(0, RTL_ISOLATE);
                    parts[last].push(PDI);
                    let candidate = parts.concat();
                    if balanced(&candidate, kind) {
                        raw = candidate;
                    }
                }
            }
        }
        output.push_str(&raw);
        if let Some(delimiter) = breaks.get(i) {
            output.push_str(delimiter);
        }
    }
    Ok(output)
}
