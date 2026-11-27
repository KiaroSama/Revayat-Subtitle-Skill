use std::borrow::Cow;
use std::collections::HashMap;

use unicode_bidi::{BidiClass, InitialInfo, Level, bidi_class};

use super::{
    Regex,
    bidi_prose::{AssContext, project},
    python_space,
    tokens::{Piece, pieces},
};
use Piece::Text;

const RLM: char = '\u{200f}';
const LRM: char = '\u{200e}';
const LRE: char = '\u{202a}';
const PDF: char = '\u{202c}';
pub(super) const CONTROLS: &str = "\u{061c}\u{200e}\u{200f}\u{202a}\u{202b}\u{202c}\u{202d}\u{202e}\u{2066}\u{2067}\u{2068}\u{2069}";

fn strong(c: char, runtime_classes: Option<&HashMap<char, BidiClass>>) -> Option<BidiClass> {
    if CONTROLS.contains(c) {
        return None;
    }
    if let Some(classes) = runtime_classes {
        return classes.get(&c).copied();
    }
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
    has_rtl_with_classes(text, None)
}

pub(super) fn has_rtl_with_classes(
    text: &str,
    runtime_classes: Option<&HashMap<char, BidiClass>>,
) -> bool {
    text.chars().any(|c| {
        matches!(c, '\u{0620}'..='\u{063f}' | '\u{0641}'..='\u{064a}' | '\u{0660}'..='\u{0669}'
            | '\u{066e}'..='\u{06d3}' | '\u{06f0}'..='\u{06fc}')
            || matches!(
                strong(c, runtime_classes),
                Some(BidiClass::R | BidiClass::AL | BidiClass::AN)
            )
    })
}

pub(super) fn has_ltr(text: &str) -> bool {
    has_ltr_with_classes(text, None)
}

pub(super) fn has_ltr_with_classes(
    text: &str,
    runtime_classes: Option<&HashMap<char, BidiClass>>,
) -> bool {
    text.chars().any(|c| {
        matches!(
            strong(c, runtime_classes),
            Some(BidiClass::L | BidiClass::EN)
        )
    })
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

pub(crate) const RTL_ISOLATE: &str = "\u{200f}\u{2067}";
pub(crate) const PDI: char = '\u{2069}';

pub(crate) fn wrapper_fits(visible_prose: &str, rtl: bool) -> bool {
    use BidiClass::{B, FSI, LRE, LRI, LRO, PDF, PDI, RLE, RLI, RLO};

    let base = if rtl { Level::rtl() } else { Level::ltr() };
    // InitialInfo resolves FSI from its own first strong character, skipping nested isolates.
    let info = InitialInfo::new(visible_prose, Some(base));
    let mut scopes = Vec::<(Level, bool)>::new();
    for (at, _) in visible_prose.char_indices() {
        let class = info.original_classes[at];
        match class {
            B => scopes.clear(),
            PDF if scopes.last().is_some_and(|&(_, isolate)| !isolate) => {
                scopes.pop();
            }
            PDI => {
                if let Some(open) = scopes.iter().rposition(|&(_, isolate)| isolate) {
                    scopes.truncate(open);
                }
            }
            RLE | RLO | RLI | LRE | LRO | LRI | FSI => {
                let level = scopes.last().map_or(base, |&(level, _)| level);
                let next = if matches!(class, RLE | RLO | RLI) {
                    level.new_explicit_next_rtl()
                } else {
                    level.new_explicit_next_ltr()
                };
                let Ok(next) = next else { return false };
                scopes.push((next, matches!(class, RLI | LRI | FSI)));
            }
            _ => {}
        }
    }
    true
}

fn readable(line: &[Token<'_>], kind: &str, mut context: AssContext) -> Result<String, String> {
    let text = line
        .iter()
        .map(|(_, text)| text.as_ref())
        .collect::<String>();
    project(&text, kind, &mut context)
}

pub(crate) fn has_outer_rtl_isolate(mut text: impl Iterator<Item = char>) -> bool {
    if text.next() != Some(RLM) || text.next() != Some('\u{2067}') {
        return false;
    }
    let mut depth = 1;
    while let Some(ch) = text.next() {
        match ch {
            '\u{2066}'..='\u{2068}' => depth += 1,
            PDI => {
                depth -= 1;
                if depth == 0 {
                    return text.next().is_none();
                }
            }
            _ => (),
        }
        if depth > 125 {
            return false;
        }
    }
    false
}

fn flush(
    line: &mut Vec<Token<'_>>,
    output: &mut String,
    kind: &str,
    rtl: bool,
    force: bool,
    runtime_classes: Option<&HashMap<char, BidiClass>>,
    context: &mut AssContext,
) -> Result<(), String> {
    let line_context = *context;
    let original = line
        .iter()
        .map(|(_, text)| text.as_ref())
        .collect::<String>();
    project(&original, kind, context)?;
    let has_rtl = |text: &str| has_rtl_with_classes(text, runtime_classes);
    let has_ltr = |text: &str| has_ltr_with_classes(text, runtime_classes);
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
        if rtl
            && has_outer_rtl_isolate(
                line[first..=last]
                    .iter()
                    .filter(|(piece, _)| *piece == Text)
                    .flat_map(|(_, value)| value.chars()),
            )
        {
            for (_, value) in line.drain(..) {
                output.push_str(&value);
            }
            return Ok(());
        }
        for (prefix, suffix) in [
            ("\u{202b}\u{200f}", "\u{200f}\u{202c}"),
            ("\u{202a}\u{200e}", "\u{200e}\u{202c}"),
            ("\u{202b}", "\u{202c}"),
            ("\u{200f}", "\u{200f}"),
            ("\u{200e}", "\u{200e}"),
        ] {
            if line[first].1.starts_with(prefix)
                && line[last].1.ends_with(suffix)
                && (first != last || line[first].1.len() >= prefix.len() + suffix.len())
            {
                let original_first = line[first].1.clone();
                let original_last = line[last].1.clone();
                line[first].1.to_mut().drain(..prefix.len());
                let end = line[last].1.len() - suffix.len();
                line[last].1.to_mut().truncate(end);
                // The endpoints may belong to sibling scopes, not one outer wrapper.
                if suffix.ends_with(PDF)
                    && validate(
                        &line.iter().map(|(_, s)| s.as_ref()).collect::<String>(),
                        kind,
                    )
                    .is_err()
                {
                    line[first].1 = original_first;
                    line[last].1 = original_last;
                }
                break;
            }
        }
        if force || indices.iter().any(|&i| has_rtl(&line[i].1)) {
            let mixed = indices.iter().any(|&i| has_ltr(&line[i].1));
            // Do not enclose half of an intentional scope that crosses a display break.
            let balanced = validate(
                &line.iter().map(|(_, s)| s.as_ref()).collect::<String>(),
                kind,
            )
            .is_ok();
            let mut isolated = false;
            if rtl && balanced {
                let original_first = line[first].1.clone();
                let original_last = line[last].1.clone();
                line[first].1.to_mut().insert_str(0, RTL_ISOLATE);
                line[last].1.to_mut().push(PDI);
                isolated = wrapper_fits(&readable(line, kind, line_context)?, true)
                    && validate(
                        &line.iter().map(|(_, s)| s.as_ref()).collect::<String>(),
                        kind,
                    )
                    .is_ok();
                if !isolated {
                    // Boundary marks preserve input already at the supported nesting limit.
                    line[first].1 = original_first;
                    line[last].1 = original_last;
                }
            }
            if !isolated {
                let mark = if rtl { RLM } else { LRM };
                line[first].1.to_mut().insert(0, mark);
                line[last].1.to_mut().push(mark);
                if !rtl && mixed && balanced {
                    let original_first = line[first].1.clone();
                    let original_last = line[last].1.clone();
                    line[first].1.to_mut().insert(0, LRE);
                    line[last].1.to_mut().push(PDF);
                    if !wrapper_fits(&readable(line, kind, line_context)?, false)
                        || validate(
                            &line.iter().map(|(_, s)| s.as_ref()).collect::<String>(),
                            kind,
                        )
                        .is_err()
                    {
                        line[first].1 = original_first;
                        line[last].1 = original_last;
                    }
                }
            }
        }
    }
    for (_, value) in line.drain(..) {
        output.push_str(&value);
    }
    Ok(())
}

pub(super) fn normalize(
    text: &str,
    kind: &str,
    direction: &str,
    force: bool,
    runtime_classes: Option<&HashMap<char, BidiClass>>,
) -> Result<String, String> {
    normalize_with_context(
        text,
        kind,
        direction,
        force,
        runtime_classes,
        AssContext::default(),
    )
}

pub(super) fn normalize_with_context(
    text: &str,
    kind: &str,
    direction: &str,
    force: bool,
    runtime_classes: Option<&HashMap<char, BidiClass>>,
    mut context: AssContext,
) -> Result<String, String> {
    if !matches!(direction, "rtl" | "ltr") {
        return Err("ValueError: Paragraph direction must be ltr or rtl".into());
    }
    validate(text, kind)?;
    let mut output = String::with_capacity(text.len());
    let mut line = Vec::new();
    let runs = text_runs(text, kind)?;
    let mut delimiter_context = context;
    let breaks = if kind == "ass" {
        pattern!(Regex, r"\\[Nn]")
    } else {
        pattern!(Regex, "\n")
    };
    for (piece, value) in &runs {
        if *piece != Text {
            if kind == "ass" && *piece == Piece::Tag {
                delimiter_context.tag(value)?;
            }
            line.push((*piece, Cow::Borrowed(value.as_ref())));
            continue;
        }
        let mut add = |part, is_break| -> Result<(), String> {
            if is_break {
                flush(
                    &mut line,
                    &mut output,
                    kind,
                    direction == "rtl",
                    force,
                    runtime_classes,
                    &mut context,
                )?;
                output.push_str(part);
            } else {
                line.push((Text, Cow::Borrowed(part)));
            }
            Ok(())
        };
        let mut cursor = 0;
        for delimiter in breaks.find_iter(value) {
            if kind == "ass"
                && delimiter.as_str() == r"\n"
                && !delimiter_context.soft_break_is_newline()
            {
                continue;
            }
            add(&value[cursor..delimiter.start()], false)?;
            add(delimiter.as_str(), true)?;
            cursor = delimiter.end();
        }
        add(&value[cursor..], false)?;
    }
    flush(
        &mut line,
        &mut output,
        kind,
        direction == "rtl",
        force,
        runtime_classes,
        &mut context,
    )?;
    validate(&output, kind)?;
    Ok(output)
}
