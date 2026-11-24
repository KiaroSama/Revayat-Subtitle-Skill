use std::collections::BTreeSet;

use super::{FancyRegex, Regex, python_space, regex_error};

const TAG: &str = r"[a-zA-Z][^\t\n\r\x0c />]*";
const GAP: &str = r"[\t\n\r\x0c /]";
const SPACE: &str = r"[\t\n\r\x0c ]";
const ATTR: &str = r#"(?<=['"\t\n\r\x0c /])[^\t\n\r\x0c />][^\t\n\r\x0c /=>]*"#;
const VALUE: &str = r#"'[^']*'|"[^"]*"|(?!['"])[^>\t\n\r\x0c ]*"#;

fn tag_end(input: &str) -> Result<Option<usize>, String> {
    let end = pattern!(
        FancyRegex,
        &format!(r"\A{TAG}{GAP}*(?:{ATTR}(?:{SPACE}*={SPACE}*(?:{VALUE}))?{GAP}*)*>?")
    );
    let matched = end.find(input).map_err(regex_error)?;
    Ok(matched
        .filter(|m| m.start() == 0 && m.as_str().ends_with('>'))
        .map(|m| m.end()))
}

fn numeric_reference(digits: &str, radix: u32) -> Result<Option<char>, String> {
    if radix == 10 && digits.len() > 4300 {
        return Err(format!(
            "ValueError: Exceeds the limit (4300 digits) for integer string conversion: value has {} digits; use sys.set_int_max_str_digits() to increase the limit",
            digits.len()
        ));
    }
    let code = u32::from_str_radix(digits.trim_start_matches('0'), radix).unwrap_or(
        if digits.bytes().all(|c| c == b'0') {
            0
        } else {
            u32::MAX
        },
    );
    Ok(match code {
        0 => Some('\u{fffd}'),
        0x80..=0x9f => {
            const C1: [char; 32] = [
                '\u{20ac}', '\u{81}', '\u{201a}', '\u{192}', '\u{201e}', '\u{2026}', '\u{2020}',
                '\u{2021}', '\u{2c6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8d}',
                '\u{17d}', '\u{8f}', '\u{90}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}',
                '\u{2022}', '\u{2013}', '\u{2014}', '\u{2dc}', '\u{2122}', '\u{161}', '\u{203a}',
                '\u{153}', '\u{9d}', '\u{17e}', '\u{178}',
            ];
            Some(C1[(code - 0x80) as usize])
        }
        1..=8 | 11 | 14..=31 | 127 | 0xfdd0..=0xfdef => None,
        0..=0x10ffff if code & 0xffff >= 0xfffe => None,
        _ => Some(char::from_u32(code).unwrap_or('\u{fffd}')),
    })
}

fn named_reference(body: &str) -> Option<&'static str> {
    // WHATWG legacy names recognized without a semicolon in an attribute.
    const LEGACY: &str = " AElig AMP Aacute Acirc Agrave Aring Atilde Auml COPY Ccedil ETH Eacute Ecirc Egrave Euml GT Iacute Icirc Igrave Iuml LT Ntilde Oacute Ocirc Ograve Oslash Otilde Ouml QUOT REG THORN Uacute Ucirc Ugrave Uuml Yacute aacute acirc acute aelig agrave amp aring atilde auml brvbar ccedil cedil cent copy curren deg divide eacute ecirc egrave eth euml frac12 frac14 frac34 gt iacute icirc iexcl igrave iquest iuml laquo lt macr micro middot nbsp not ntilde oacute ocirc ograve ordf ordm oslash otilde ouml para plusmn pound quot raquo reg sect shy sup1 sup2 sup3 szlig thorn times uacute ucirc ugrave uml uuml yacute yen yuml ";
    let name = if let Some(name) = body.strip_suffix(';') {
        name
    } else if LEGACY.split_ascii_whitespace().any(|name| name == body) {
        body
    } else {
        return None;
    };
    let table = &html_escape::NAMED_ENTITIES;
    let i = table
        .binary_search_by(|&(key, _)| key.cmp(name.as_bytes()))
        .ok()?;
    Some(table[i].1)
}

fn attribute_value(value: &str, legacy_attributes: bool) -> Result<String, String> {
    let refs = pattern!(
        Regex,
        r"&(?:#[0-9]+|#[xX][0-9a-fA-F]+|[a-zA-Z][a-zA-Z0-9]*)[;=]?"
    );
    let mut output = String::with_capacity(value.len());
    let mut cursor = 0;
    for reference in refs.find_iter(value) {
        output.push_str(&value[cursor..reference.start()]);
        cursor = reference.end();
        let body = &reference.as_str()[1..];
        if let Some(numeric) = body.strip_prefix('#') {
            let bare = numeric.trim_end_matches([';', '=']);
            let (digits, radix) = bare
                .strip_prefix(['x', 'X'])
                .map_or((bare, 10), |hex| (hex, 16));
            output.extend(numeric_reference(digits, radix)?);
            if body.ends_with('=') {
                output.push('=');
            }
        } else if let Some(decoded) = named_reference(body) {
            output.push_str(decoded);
        } else if legacy_attributes {
            // Earlier supported Python interpreters use html.unescape here,
            // which also accepts the longest legacy name prefix before '=' or
            // alphanumeric suffixes. New HTMLParser versions preserve those.
            if let Some((prefix, decoded)) = (2..body.len())
                .rev()
                .find_map(|length| named_reference(&body[..length]).map(|value| (length, value)))
            {
                output.push_str(decoded);
                output.push_str(&body[prefix..]);
            } else {
                output.push_str(reference.as_str());
            }
        } else {
            output.push_str(reference.as_str());
        }
    }
    output.push_str(&value[cursor..]);
    Ok(output)
}

fn start_tag(
    tag: &str,
    cursor: usize,
    names: &mut BTreeSet<String>,
    legacy_attributes: bool,
) -> Result<Option<String>, String> {
    let finder = pattern!(FancyRegex, &format!(r"\A({TAG})(?:{SPACE}|/(?!>))*"));
    let attrs = pattern!(
        FancyRegex,
        &format!(r"({ATTR})(?:{SPACE}*={SPACE}*({VALUE}))?(?:{SPACE}|/(?!>))*")
    );
    let Some(start) = finder.captures(&tag[cursor..]).map_err(regex_error)? else {
        return Ok(None);
    };
    let (Some(matched), Some(name)) = (start.get(0), start.get(1)) else {
        return Ok(None);
    };
    let name = name.as_str().to_ascii_lowercase();
    let mut offset = cursor + matched.end();
    let mut faces = Vec::new();
    while offset < tag.len() {
        let Some(attr) = attrs.captures_from_pos(tag, offset).map_err(regex_error)? else {
            break;
        };
        let (Some(matched), Some(key)) = (attr.get(0), attr.get(1)) else {
            break;
        };
        if matched.start() != offset {
            break;
        }
        if let Some(value) = attr.get(2) {
            let raw = value.as_str();
            let raw = raw
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .or_else(|| raw.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                .unwrap_or(raw);
            let decoded = attribute_value(raw, legacy_attributes)?;
            if name == "font" && key.as_str().eq_ignore_ascii_case("face") {
                let face = decoded.trim_matches(python_space);
                if !face.is_empty() {
                    faces.push(face.to_owned());
                }
            }
        }
        offset = matched.end();
    }
    match tag[offset..].trim_matches(python_space) {
        ">" => {
            names.extend(faces);
            Ok(Some(name))
        }
        "/>" => {
            names.extend(faces);
            Ok(None)
        }
        _ => Ok(None),
    }
}

fn char_ref(input: &str) -> Option<usize> {
    let numeric = pattern!(Regex, r"\A&#(?:[0-9]+|[xX][0-9a-fA-F]+)[^0-9a-fA-F]");
    let named = pattern!(Regex, r"\A&[a-zA-Z][-.a-zA-Z0-9]*[^a-zA-Z0-9]");
    if let Some(m) = numeric.find(input).or_else(|| named.find(input)) {
        return Some(if m.as_str().ends_with(';') {
            m.end()
        } else {
            m.end() - m.as_str().chars().next_back()?.len_utf8()
        });
    }
    let incomplete = pattern!(Regex, r"\A&(?:[a-zA-Z]|#(?:[0-9]|[xX][0-9a-fA-F]))");
    if incomplete.is_match(input)
        || input.chars().take(4).count() <= if input.starts_with("&#") { 3 } else { 1 }
    {
        None
    } else {
        Some(if input.starts_with("&#") { 2 } else { 1 })
    }
}

pub(super) fn names(mut input: &str, legacy_attributes: bool) -> Result<BTreeSet<String>, String> {
    let comment_close = pattern!(Regex, "--!?>");
    let mut names = BTreeSet::new();
    let mut raw = None::<String>;
    while !input.is_empty() {
        let interesting = |other: &str| {
            if let Some(raw) = &raw {
                if other.starts_with('&') {
                    return matches!(raw.as_str(), "textarea" | "title");
                }
                other.strip_prefix("</").is_some_and(|tail| {
                    tail.get(..raw.len())
                        .is_some_and(|n| n.eq_ignore_ascii_case(raw))
                        && tail
                            .as_bytes()
                            .get(raw.len())
                            .is_some_and(|b| b"\t\n\r\x0c />".contains(b))
                })
            } else {
                true
            }
        };
        let Some(start) = input
            .match_indices(['<', '&'])
            .map(|(i, _)| i)
            .find(|&i| interesting(&input[i..]))
        else {
            break;
        };
        input = &input[start..];
        let skip = if input.starts_with('&') {
            char_ref(input)
        } else if let Some(comment) = input.strip_prefix("<!--") {
            if comment.starts_with('>') {
                Some(5)
            } else if comment.starts_with("->") {
                Some(6)
            } else {
                comment_close.find(comment).map(|m| m.end() + 4)
            }
        } else if let Some(cdata) = input.strip_prefix("<![CDATA[") {
            cdata.find("]]>").map(|i| i + 12)
        } else if input.starts_with("<!") || input.starts_with("<?") {
            input.find('>').map(|i| i + 1)
        } else if let Some(tail) = input.strip_prefix("</") {
            if tail.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) {
                tag_end(tail)?.map(|end| {
                    raw = None;
                    end + 2
                })
            } else {
                tail.find('>').map(|i| i + 3)
            }
        } else if input.as_bytes().get(1).is_some_and(u8::is_ascii_alphabetic) {
            let Some(end) = tag_end(&input[1..])? else {
                break;
            };
            if let Some(name) = start_tag(&input[..end + 1], 1, &mut names, legacy_attributes)? {
                match name.as_str() {
                    "plaintext" => break,
                    "script" | "style" | "xmp" | "iframe" | "noembed" | "noframes" | "textarea"
                    | "title" => raw = Some(name),
                    _ => (),
                }
            }
            Some(end + 1)
        } else {
            Some(1)
        };
        let Some(skip) = skip else {
            break;
        };
        input = &input[skip..];
    }
    Ok(names)
}
