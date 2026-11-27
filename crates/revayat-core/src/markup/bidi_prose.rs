//! Renderer-visible prose used only for directional-control analysis.

use super::tokens::{Piece, drawing_in, overrides, pieces_with_drawing};

#[derive(Clone, Copy, Default)]
pub(crate) struct AssContext {
    track_wrap_style: i32,
    wrap_style: i32,
    drawing: bool,
}

impl AssContext {
    pub(crate) fn new(track_wrap_style: i32, effect: &str) -> Self {
        // libass applies a valid Banner effect before parsing inline overrides.
        let banner = effect
            .trim_start_matches([' ', '\t'])
            .starts_with("Banner;");
        Self {
            track_wrap_style,
            wrap_style: if banner { 2 } else { track_wrap_style },
            drawing: false,
        }
    }

    pub(crate) fn tag(&mut self, block: &str) -> Result<(), String> {
        self.drawing = drawing_in(block, self.drawing)?;
        overrides(block, |name, range| {
            if name == "q" {
                let argument = block[range].trim_matches([' ', '\t']);
                let value = decimal_prefix(argument);
                self.wrap_style = if argument.is_empty() || !(0..=3).contains(&value) {
                    self.track_wrap_style
                } else {
                    value
                };
            }
            // Style resets do not reset libass's wrap mode.
            Ok(())
        })
    }

    pub(crate) fn soft_break_is_newline(&self) -> bool {
        self.wrap_style == 2
    }

    fn prose(&self, text: &str, output: &mut String) {
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                let decoded = match chars.peek() {
                    Some('h') => Some('\u{00a0}'),
                    Some('N') => Some('\n'),
                    Some('n') => Some(if self.soft_break_is_newline() {
                        '\n'
                    } else {
                        ' '
                    }),
                    Some('{') => Some('{'),
                    Some('}') => Some('}'),
                    _ => None,
                };
                if let Some(decoded) = decoded {
                    chars.next();
                    output.push(decoded);
                    continue;
                }
            }
            output.push(if c == '\t' { ' ' } else { c });
        }
    }
}

fn decimal_prefix(value: &str) -> i32 {
    let value = value.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let negative = value.starts_with('-');
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    // Only 0..=3 are meaningful for q; larger strtoll results all select the default.
    let number = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0i32, |n, c| (n * 10 + i32::from(c - b'0')).min(4));
    if negative { -number } else { number }
}

/// Match libass's signed, modulo-2^32 Script Info integer prefixes.
pub(crate) fn header_integer(value: &str) -> i32 {
    let (radix, digits) = ["&H", "&h", "0x", "0X"]
        .iter()
        .find_map(|prefix| value.strip_prefix(prefix))
        .map_or((10u32, value), |hex| (16, hex));
    let digits = digits.trim_start_matches([' ', '\t']);
    let negative = digits.starts_with('-');
    let digits = digits.strip_prefix(['+', '-']).unwrap_or(digits);
    let digits = if radix == 16 {
        digits
            .strip_prefix("0x")
            .or_else(|| digits.strip_prefix("0X"))
            .unwrap_or(digits)
    } else {
        digits
    };
    let number = digits
        .chars()
        .map_while(|c| c.to_digit(radix))
        .fold(0u32, |n, digit| n.wrapping_mul(radix).wrapping_add(digit));
    (if negative {
        number.wrapping_neg()
    } else {
        number
    }) as i32
}

pub(crate) fn project(text: &str, kind: &str, context: &mut AssContext) -> Result<String, String> {
    let mut output = String::with_capacity(text.len());
    pieces_with_drawing(
        text,
        kind,
        kind == "ass" && context.drawing,
        |piece, value| {
            match piece {
                Piece::Text if kind == "ass" => context.prose(value, &mut output),
                Piece::Text => output.push_str(value),
                Piece::Tag if kind == "ass" => context.tag(value)?,
                _ => {}
            }
            Ok(())
        },
    )?;
    Ok(output)
}
