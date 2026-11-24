use std::borrow::Cow;

use unicode_bidi::{BidiClass, bidi_class};

use super::{
    Regex, python_space,
    tokens::{Piece, pieces},
};
use Piece::Text;

const RLM: char = '\u{200f}';
const LRM: char = '\u{200e}';
const RLE: char = '\u{202b}';
const LRE: char = '\u{202a}';
const PDF: char = '\u{202c}';
pub(super) const CONTROLS: &str = "\u{061c}\u{200e}\u{200f}\u{202a}\u{202b}\u{202c}\u{202d}\u{202e}\u{2066}\u{2067}\u{2068}\u{2069}";

fn strong(c: char) -> Option<BidiClass> {
    // Unicode supplies default bidi classes for unassigned codepoints; Python returns no class.
    let assigned = pattern!(Regex, r"\A[\p{Age=15.1}&&\P{Cn}]\z");
    (!CONTROLS.contains(c) && (c.is_ascii() || assigned.is_match(c.encode_utf8(&mut [0; 4])))).then(
        || match c {
            // CPython 3.13 uses Unicode 15.1; these classes changed in Unicode 16.
            '\u{1171e}' => BidiClass::NSM,
            '\u{1d6c1}' | '\u{1d6fb}' | '\u{1d735}' | '\u{1d76f}' | '\u{1d7a9}' => BidiClass::L,
            _ => bidi_class(c),
        },
    )
}

pub(super) fn has_rtl(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(c, '\u{0620}'..='\u{063f}' | '\u{0641}'..='\u{064a}' | '\u{0660}'..='\u{0669}'
            | '\u{066e}'..='\u{06d3}' | '\u{06f0}'..='\u{06fc}')
            || matches!(
                strong(c),
                Some(BidiClass::R | BidiClass::AL | BidiClass::AN)
            )
    })
}

pub(super) fn has_ltr(text: &str) -> bool {
    text.chars()
        .any(|c| matches!(strong(c), Some(BidiClass::L | BidiClass::EN)))
}

pub(super) fn validate(text: &str, kind: &str) -> Result<(), String> {
    let mut stack = Vec::new();
    pieces(text, kind, |piece, value| {
        if piece != Text {
            return Ok(());
        }
        for c in value.chars() {
            match c {
                '\u{202a}' | '\u{202b}' | '\u{202d}' | '\u{202e}' => stack.push(false),
                '\u{2066}'..='\u{2068}' => stack.push(true),
                PDF => {
                    if stack.pop() != Some(false) {
                        return Err("ValueError: Unbalanced bidi embedding terminator".into());
                    }
                }
                '\u{2069}' => loop {
                    match stack.pop() {
                        Some(true) => break,
                        Some(false) => continue,
                        None => return Err("ValueError: Unbalanced bidi isolate terminator".into()),
                    }
                },
                _ => (),
            }
            if stack.len() > 125 {
                return Err("ValueError: Bidi nesting exceeds the supported limit".into());
            }
        }
        Ok(())
    })?;
    if !stack.is_empty() {
        return Err("ValueError: Unclosed bidi embedding or isolate".into());
    }
    Ok(())
}

type Token<'a> = (Piece, Cow<'a, str>);

fn text_runs<'a>(text: &'a str, kind: &str) -> Result<Vec<Token<'a>>, String> {
    let mut runs: Vec<Token<'_>> = Vec::new();
    pieces(text, kind, |piece, value| {
        if let (Text, Some((Text, preceding))) = (piece, runs.last_mut()) {
            preceding.to_mut().push_str(value);
        } else {
            runs.push((piece, Cow::Borrowed(value)));
        }
        Ok(())
    })?;
    Ok(runs)
}

fn flush(line: &mut Vec<Token<'_>>, output: &mut String, kind: &str, rtl: bool, force: bool) {
    let indices = line
        .iter()
        .enumerate()
        .filter_map(|(i, (piece, value))| {
            (*piece == Text && !value.trim_matches(python_space).is_empty()).then_some(i)
        })
        .collect::<Vec<_>>();
    if let (Some(&first), Some(&last)) = (indices.first(), indices.last())
        && (force || indices.iter().any(|&i| has_rtl(&line[i].1)))
    {
        for (prefix, suffix) in [
            ("\u{202b}\u{200f}", "\u{200f}\u{202c}"),
            ("\u{202a}\u{200e}", "\u{200e}\u{202c}"),
            ("\u{200f}", "\u{200f}"),
            ("\u{200e}", "\u{200e}"),
        ] {
            if line[first].1.starts_with(prefix)
                && line[last].1.ends_with(suffix)
                && (first != last || line[first].1.len() >= prefix.len() + suffix.len())
            {
                line[first].1.to_mut().drain(..prefix.len());
                let end = line[last].1.len() - suffix.len();
                line[last].1.to_mut().truncate(end);
                break;
            }
        }
        if force || indices.iter().any(|&i| has_rtl(&line[i].1)) {
            let mixed = indices.iter().any(|&i| has_ltr(&line[i].1));
            let terminal = rtl
                && line[last]
                    .1
                    .trim_end_matches(python_space)
                    .ends_with(['.', '،', '؟', '!', '…', ':', '؛']);
            // Marks may cross display lines; embeddings must not enclose half an isolate.
            let embedding = (mixed || terminal)
                && validate(
                    &line.iter().map(|(_, s)| s.as_ref()).collect::<String>(),
                    kind,
                )
                .is_ok();
            let mark = if rtl { RLM } else { LRM };
            line[first].1.to_mut().insert(0, mark);
            line[last].1.to_mut().push(mark);
            if embedding {
                line[first]
                    .1
                    .to_mut()
                    .insert(0, if rtl { RLE } else { LRE });
                line[last].1.to_mut().push(PDF);
            }
        }
    }
    for (_, value) in line.drain(..) {
        output.push_str(&value);
    }
}

pub(super) fn normalize(
    text: &str,
    kind: &str,
    direction: &str,
    force: bool,
) -> Result<String, String> {
    if !matches!(direction, "rtl" | "ltr") {
        return Err("ValueError: Paragraph direction must be ltr or rtl".into());
    }
    validate(text, kind)?;
    let mut output = String::with_capacity(text.len());
    let mut line = Vec::new();
    let runs = text_runs(text, kind)?;
    let breaks = if kind == "ass" {
        pattern!(Regex, r"\\[Nn]")
    } else {
        pattern!(Regex, "\n")
    };
    for (piece, value) in &runs {
        if *piece != Text {
            line.push((*piece, Cow::Borrowed(value.as_ref())));
            continue;
        }
        let mut add = |part| {
            if matches!(part, "\n" | r"\N" | r"\n") {
                flush(&mut line, &mut output, kind, direction == "rtl", force);
                output.push_str(part);
            } else {
                line.push((Text, Cow::Borrowed(part)));
            }
        };
        let mut cursor = 0;
        for delimiter in breaks.find_iter(value) {
            add(&value[cursor..delimiter.start()]);
            add(delimiter.as_str());
            cursor = delimiter.end();
        }
        add(&value[cursor..]);
    }
    flush(&mut line, &mut output, kind, direction == "rtl", force);
    validate(&output, kind)?;
    Ok(output)
}
