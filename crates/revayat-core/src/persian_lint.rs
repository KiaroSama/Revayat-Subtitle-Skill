//! Persian text normalization, style checks, and line-level changes.
#![forbid(unsafe_code)]

use fancy_regex::{Captures, Match, Regex};
use serde_json::{Map, Value, json};
use std::sync::OnceLock;

const PL: &str = "ء-يٮ-ەۺ-ۼۿ";
const PLD: &str = "ء-يٮ-ەۺ-ۼۿ\u{064b}-\u{0652}\u{0670}";
const AR: &str = "ء-يٮ-ەۺ-ۼۿ۰-۹٠-٩\u{200c}";
const ZWNJ: char = '\u{200c}';
const SENTINEL: char = '\0';

fn compile(pattern: &str) -> fancy_regex::Result<Regex> {
    // Python adds four controls to whitespace and excludes marks and joiners from \w.
    const SPACE: &str = r"\p{White_Space}\x1c-\x1f";
    const WORD: &str = r"[\p{L}\p{N}_]";
    Regex::new(
        &pattern
            .replace(r"[^\s", &format!("[^{SPACE}"))
            .replace(r"\s", &format!("[{SPACE}]"))
            .replace(r"\S", &format!("[^{SPACE}]"))
            .replace(
                r"\b",
                &format!("(?:(?<={WORD})(?!{WORD})|(?<!{WORD})(?={WORD}))"),
            ),
    )
}

fn regex_error(error: &fancy_regex::Error) -> String {
    format!("RuntimeError: {error}")
}

macro_rules! regex {
    ($pattern:expr) => {{
        static REGEX: OnceLock<fancy_regex::Result<Regex>> = OnceLock::new();
        REGEX
            .get_or_init(|| compile($pattern))
            .as_ref()
            .map_err(regex_error)?
    }};
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(number) if number.is_f64() => "float",
        Value::Number(_) => "int",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(map) if map.len() == 1 => match map.keys().next().map(String::as_str) {
            Some("$int") => "int",
            Some("$num") => "float",
            Some("$tuple") => "tuple",
            Some("$set") => "set",
            Some("$bytes") => "bytes",
            _ => "dict",
        },
        Value::Object(_) => "dict",
    }
}

fn regex_text(value: &Value) -> Result<&str, String> {
    value.as_str().ok_or_else(|| match type_name(value) {
        "bytes" => "TypeError: cannot use a string pattern on a bytes-like object".into(),
        name => format!("TypeError: expected string or bytes-like object, got '{name}'"),
    })
}

fn mask(text: &str) -> Result<(String, Vec<&str>), String> {
    let protect = regex!(concat!(
        r"(?sm)```.*?```|`[^`\n]*`|^(?: {4,}|\t)\S[^\n]*$|(?:https?://|www\.)\S+",
        r"|[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}|\bv\d+(?:\.\d+)+\b|\b\d+\.\d+\.\d+\b|\x00",
    ));
    let mut spans = Vec::new();
    let mut result = String::with_capacity(text.len());
    let mut end = 0;
    for found in protect.find_iter(text) {
        let found = found.map_err(|e| regex_error(&e))?;
        result.push_str(&text[end..found.start()]);
        result.push(SENTINEL);
        spans.push(found.as_str());
        end = found.end();
    }
    result.push_str(&text[end..]);
    Ok((result, spans))
}

fn unmask(text: &str, spans: Vec<&str>) -> String {
    let mut parts = text.split(SENTINEL);
    let mut spans = spans.into_iter();
    let mut result = parts.next().unwrap_or_default().to_owned();
    for part in parts {
        result.push_str(spans.next().unwrap_or_default());
        result.push_str(part);
    }
    result
}

fn rewrite(
    text: &mut String,
    pattern: &Regex,
    mut append: impl FnMut(&Captures<'_, str>, &mut String) -> bool,
) -> Result<usize, String> {
    let mut count = 0;
    let result = pattern
        .try_replacen(text, 0, |captures: &Captures<'_, str>| {
            let mut replacement = String::new();
            if append(captures, &mut replacement) {
                count += 1;
            } else if let Some(found) = captures.get(0) {
                replacement.push_str(found.as_str());
            }
            replacement
        })
        .map_err(|e| regex_error(&e))?;
    if let std::borrow::Cow::Owned(result) = result {
        *text = result;
    }
    Ok(count)
}

fn translate(text: &mut String, mut translate: impl FnMut(char) -> Option<char>) -> usize {
    let mut count = 0;
    *text = text
        .chars()
        .map(|ch| match translate(ch) {
            Some(fixed) => {
                count += 1;
                fixed
            }
            None => ch,
        })
        .collect();
    count
}

fn persian_letter(ch: char) -> bool {
    matches!(ch, 'ء'..='ي' | 'ٮ'..='ە' | 'ۺ'..='ۼ' | 'ۿ')
}

fn persian_context(ch: char) -> bool {
    persian_letter(ch) || matches!(ch, '۰'..='۹' | '٠'..='٩' | ZWNJ)
}

fn persian_line(line: &str) -> bool {
    line.chars().filter(|ch| persian_letter(*ch)).count()
        > line.bytes().filter(u8::is_ascii_alphabetic).count()
}

fn punctuation(line: &mut String, latin: char, persian: char, is_persian: bool) -> usize {
    let digit = |ch: char| ch.is_ascii_digit() || matches!(ch, '۰'..='۹');
    let mut previous = None;
    let mut count = 0;
    *line = line
        .char_indices()
        .map(|(offset, ch)| {
            let after = &line[offset + ch.len_utf8()..];
            let replace = ch == latin
                && (is_persian
                    || (previous.is_some_and(persian_context)
                        && after
                            .trim_start_matches([' ', '\t'])
                            .starts_with(persian_context)))
                && (latin != ',' || !(previous.is_some_and(digit) && after.starts_with(digit)));
            previous = Some(ch);
            if replace {
                count += 1;
                persian
            } else {
                ch
            }
        })
        .collect();
    count
}

fn persian_digits(line: &mut String) -> Result<usize, String> {
    let head = regex!(r"^\s*\d+[.)]\s")
        .find(line.as_str())
        .map_err(|e| regex_error(&e))?
        .map_or(0, |found| found.end());
    let mut rest = line[head..].to_owned();
    let mut count = 0;
    rewrite(
        &mut rest,
        regex!(r"[A-Za-z0-9]+(?:[-_][A-Za-z0-9]+)*"),
        |captures, result| {
            let token = captures.get(0).map_or("", |found| found.as_str());
            if token.bytes().any(|ch| ch.is_ascii_alphabetic()) {
                return false;
            }
            result.extend(token.chars().map(|ch| {
                if ch.is_ascii_digit() {
                    count += 1;
                    char::from_u32(ch as u32 - '0' as u32 + '۰' as u32).unwrap_or(ch)
                } else {
                    ch
                }
            }));
            true
        },
    )?;
    if count != 0 {
        line.truncate(head);
        line.push_str(&rest);
    }
    Ok(count)
}

pub fn normalize(text: Value, digits: String) -> Result<Value, String> {
    let (mut masked, spans) = mask(regex_text(&text)?)?;
    let mut stats = Map::new();
    let mut bump = |name: &str, count: usize| {
        if count != 0 {
            let previous = stats.get(name).and_then(Value::as_u64).unwrap_or(0);
            stats.insert(name.into(), json!(previous + count as u64));
        }
    };
    macro_rules! sub {
        ($text:expr, $pattern:expr, $replacement:expr, $stat:expr) => {
            bump(
                $stat,
                rewrite(&mut $text, regex!($pattern), |captures, result| {
                    captures.expand($replacement, result);
                    true
                })?,
            );
        };
    }

    bump(
        "arabic_to_persian_letters",
        translate(&mut masked, |ch| match ch {
            'ي' | 'ى' => Some('ی'),
            'ك' => Some('ک'),
            'ة' => Some('ه'),
            _ => None,
        }),
    );
    sub!(
        masked,
        r"(?m)(?:(?<=\s)|^)ـ+(?=\s|$)",
        "–",
        "kashida_dash_to_en_dash"
    );
    sub!(masked, "ـ", "", "kashida_removed");

    let doc_persian = persian_line(&masked);
    match digits.as_str() {
        "persian" => bump(
            "digits_to_persian",
            translate(&mut masked, |ch| match ch {
                '٠'..='٩' => char::from_u32(ch as u32 - '٠' as u32 + '۰' as u32),
                _ => None,
            }),
        ),
        "latin" => bump(
            "digits_to_latin",
            translate(&mut masked, |ch| match ch {
                '۰'..='۹' => char::from_u32(ch as u32 - '۰' as u32 + '0' as u32),
                '٠'..='٩' => char::from_u32(ch as u32 - '٠' as u32 + '0' as u32),
                _ => None,
            }),
        ),
        _ => {}
    }

    let mut lines = Vec::new();
    for line in masked.split('\n') {
        let mut line = line.to_owned();
        let pl = persian_line(&line);
        if digits == "persian"
            && (pl || (doc_persian && !line.bytes().any(|ch| ch.is_ascii_alphabetic())))
        {
            bump("digits_to_persian", persian_digits(&mut line)?);
        }
        sub!(line, r"(?<=[۰-۹])%", "٪", "percent_to_persian");
        bump("comma_to_persian", punctuation(&mut line, ',', '،', pl));
        bump("semicolon_to_persian", punctuation(&mut line, ';', '؛', pl));
        bump(
            "question_mark_to_persian",
            punctuation(&mut line, '?', '؟', pl),
        );
        bump(
            "quotes_to_guillemets",
            rewrite(
                &mut line,
                regex!(r#""([^"\n]*)"|“([^“”\n]*)”"#),
                |captures, result| {
                    let inner = captures
                        .get(1)
                        .or_else(|| captures.get(2))
                        .map_or("", |m| m.as_str());
                    if !pl && !persian_line(inner) {
                        return false;
                    }
                    result.push('«');
                    result.push_str(inner);
                    result.push('»');
                    true
                },
            )?,
        );
        if pl {
            sub!(
                line,
                r"(?<=\S)[ \t]+([.!:،؛؟»])",
                "$1",
                "space_before_punct"
            );
            sub!(
                line,
                r"(!)(?=[^\s»)\]\x00،؛؟!.…])",
                "$1 ",
                "space_after_punct"
            );
        }
        lines.push(line);
    }
    masked = lines.join("\n");

    sub!(
        masked,
        &format!(r"(?<=[{PL}\u{{200c}}])\.{{3,}}"),
        "…",
        "ellipsis"
    );
    sub!(
        masked,
        &format!(r"(?<![{PL}\u{{200c}}])(نمی|می)[ \t]+(?=[{PL}])"),
        "${1}\u{200c}",
        "zwnj_mi_prefix"
    );
    sub!(
        masked,
        &format!(
            r"(?<=[{PLD}])[ \t]+(هایشان|هایتان|هایمان|هایی|هایم|هایت|هایش|های|ها)(?![{PL}])(?! و هوی)"
        ),
        "\u{200c}$1",
        "zwnj_plural_ha"
    );
    sub!(
        masked,
        &format!(r"(?<=[{PLD}])[ \t]+(ترین)(?![{PL}])"),
        "\u{200c}$1",
        "zwnj_superlative"
    );
    // A failed enclitic test cannot succeed again from inside the same Persian word.
    bump(
        "zwnj_heh_enclitic",
        rewrite(
            &mut masked,
            regex!(&format!(
                r"(?<![{PL}])([{PL}]*ه)[ \t]+(اند|ایم|اید|ام|ات|اش|ای)(?![{PL}])"
            )),
            |captures, result| {
                let word = captures.get(1).map_or("", |m| m.as_str());
                if word == "ه"
                    || matches!(
                        word,
                        "که" | "چه"
                            | "نه"
                            | "به"
                            | "سه"
                            | "بله"
                            | "آنچه"
                            | "چنانچه"
                            | "اگرچه"
                            | "گرچه"
                    )
                {
                    return false;
                }
                captures.expand("${1}\u{200c}$2", result);
                true
            },
        )?,
    );
    sub!(
        masked,
        &format!(r"(?<=[{AR}])[ \t]+([،؛؟»])"),
        "$1",
        "space_before_punct"
    );
    sub!(masked, r"«[ \t]+", "«", "space_after_openquote");
    sub!(
        masked,
        r"([،؛؟])(?=[^\s»)\]\x00،؛؟!.…])",
        "$1 ",
        "space_after_punct"
    );
    sub!(masked, r"(?<=\S)[ \t]{2,}(?=\S)", " ", "collapsed_spaces");
    sub!(masked, r"[ \t]+(\n|\z)", "$1", "trailing_space");

    Ok(json!({"$tuple": [unmask(&masked, spans), stats]}))
}

const MI_WORDS: &str = "میز میزان میزبان میان میانه میانگین میانبر میدان میوه میهن
    میهمان میمون میل میلاد میلادی میلیون میلیارد میله میخ میت
    میثاق میراث میکروب میکرو مینا مینو میش میگو میکده میکس
    میثم میرزا میر میگرن میناکاری میخانه میخک میدانی میلی میکروفون
    میکروسکوپ میکروفر میلیمتر میلی‌متر میانی میدانها میوه‌ها میزها میلها میخها
    میسر میکائیل میناب میامی میگل میلان میکل میکی میترا میثاق‌نامه";

const STYLE_FLAGS: &[(&str, &str)] = &[
    (r"می‌باشد|می‌باشند|میباشد", "«می‌باشد» -> «است / هستند»"),
    (
        r"می‌گردد|می‌گردند",
        "«می‌گردد» as a passive helper -> «می‌شود» or an active verb",
    ),
    (
        r"مورد\s+\S+\s+قرار\s+(?:می‌گیرد|گرفت|گرفته|داد|می‌دهد)",
        "«مورد … قرار گرفتن» -> plain verb (استفاده می‌شود، بررسی کردند)",
    ),
    (r"\bدارای\b", "«دارای X است» -> «X دارد»"),
    (
        r"\bتوسط\b",
        "«توسط» agent passive -> active sentence (او نوشت)",
    ),
    (
        r"\bبه عنوان\b|\bبه‌عنوان\b",
        "«به عنوان» is often a calque of English 'as'; try dropping it",
    ),
    (
        r"\bبه منظور\b|\bبه‌منظور\b|\bدر راستای\b|\bدر جهت\b",
        "bureaucratic connector -> «برای»",
    ),
    (
        r"لازم به ذکر است|شایان ذکر است|قابل ذکر است|قابل توجه است که",
        "announcement filler; delete it and state the point",
    ),
    (
        r"نه تنها .{0,60}? بلکه",
        "«نه تنها … بلکه» is over-used by models; try a plain sentence",
    ),
    (
        r"\bدر نهایت\b|\bدر پایان\b|\bبه طور کلی\b|\bدر مجموع\b",
        "summary connector; usually delete",
    ),
    (
        r"\bاین امر\b|\bاین موضوع\b|\bاین مسئله\b",
        "abstract subject; name the concrete thing",
    ),
    (
        r"\bایفا می‌کند\b|\bنقش (?:مهم|کلیدی|حیاتی)",
        "«نقش مهمی ایفا می‌کند» is the Persian 'plays a crucial role'; say what it does",
    ),
    (
        r"\bیک\s+\S+ی?\s+است\b",
        "«یک X است» often calques English 'a'; Persian usually says «Xی است» or just «X است»",
    ),
];

fn context<'a>(text: &'a str, found: Match<'_>, before: usize, after: usize) -> &'a str {
    context_at(text, found.start(), found.end(), before, after)
}

fn context_at(text: &str, start: usize, end: usize, before: usize, after: usize) -> &str {
    let start = text[..start]
        .char_indices()
        .rev()
        .nth(before - 1)
        .map_or(0, |(i, _)| i);
    let end = text[end..]
        .char_indices()
        .nth(after)
        .map_or(text.len(), |(i, _)| end + i);
    &text[start..end]
}

pub fn suggestions(text: Value, style: bool) -> Result<Value, String> {
    let (masked, _) = mask(regex_text(&text)?)?;
    let mut out = Vec::new();
    let tar = regex!(&format!(r"(?<=[{PLD}])[ \t]+تر(?![{PL}])"));
    for found in tar.find_iter(&masked) {
        let found = found.map_err(|e| regex_error(&e))?;
        out.push(json!({
            "type": "maybe_comparative_tar",
            "hint": "space+«تر»: join as «‌تر» if comparative; leave if it means 'wet'.",
            "context": context(&masked, found, 12, 3)
        }));
    }
    let mi = regex!(&format!(r"(?<![{PL}\u{{200c}}])ن?می[{PL}]+"));
    for found in mi.find_iter(&masked) {
        let found = found.map_err(|e| regex_error(&e))?;
        let word = found.as_str();
        let stem = word.strip_prefix('ن').unwrap_or(word);
        let stripped = stem.trim_end_matches('ی');
        if !MI_WORDS
            .split_whitespace()
            .any(|word| word == stem || word == stripped)
        {
            out.push(json!({
                "type": "maybe_glued_mi",
                "hint": "«می» glued to verb: insert ZWNJ (می‌…); not auto-fixed (cf. «میمون», «میز»).",
                "context": context(&masked, found, 3, 3)
            }));
        }
    }
    for (start, mark) in masked.match_indices('\u{2014}') {
        out.push(json!({
            "type": "em_dash",
            "hint": "em dash: Persian prose rarely uses it; restructure, or use a spaced en dash (–) if a dash is really needed.",
            "context": context_at(&masked, start, start + mark.len(), 12, 12).replace('\n', " ")
        }));
    }
    if style {
        static STYLE: OnceLock<fancy_regex::Result<Vec<Regex>>> = OnceLock::new();
        let patterns = STYLE
            .get_or_init(|| {
                STYLE_FLAGS
                    .iter()
                    .map(|(pattern, _)| compile(pattern))
                    .collect()
            })
            .as_ref()
            .map_err(regex_error)?;
        for (pattern, (_, hint)) in patterns.iter().zip(STYLE_FLAGS) {
            for found in pattern.find_iter(&masked) {
                let found = found.map_err(|e| regex_error(&e))?;
                out.push(json!({
                    "type": "style", "hint": hint,
                    "context": context(&masked, found, 15, 15).replace('\n', " ")
                }));
            }
        }
    }
    Ok(Value::Array(out))
}

fn split_lines(value: &Value) -> Result<std::str::Split<'_, char>, String> {
    value
        .as_str()
        .map(|text| text.split('\n'))
        .ok_or_else(|| match type_name(value) {
            "bytes" => "TypeError: a bytes-like object is required, not 'str'".into(),
            name => format!("AttributeError: '{name}' object has no attribute 'split'"),
        })
}

pub fn line_changes(original: Value, fixed: Value) -> Result<Value, String> {
    let lines = split_lines(&original)?.zip(split_lines(&fixed)?);
    Ok(Value::Array(
        lines
            .enumerate()
            .filter(|(_, (before, after))| before != after)
            .map(|(i, (before, after))| json!({"line": i + 1, "before": before, "after": after}))
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_code_and_list_markers_while_fixing_persian_prose() {
        assert_eq!(
            normalize(json!("1. كتاب ها 50% و `42`,خوب"), "persian".into()),
            Ok(json!({"$tuple": ["1. کتاب‌ها ۵۰٪ و `42`، خوب", {
                "arabic_to_persian_letters": 1,
                "digits_to_persian": 2,
                "percent_to_persian": 1,
                "comma_to_persian": 1,
                "zwnj_plural_ha": 1,
                "space_after_punct": 1
            }]}))
        );
    }

    #[test]
    fn indented_code_does_not_hide_later_persian_paragraphs() {
        assert_eq!(
            normalize(json!("    code = 1\n\nكتاب ها مي رود"), "keep".into()),
            Ok(json!({"$tuple": ["    code = 1\n\nکتاب‌ها می‌رود", {
                "arabic_to_persian_letters": 2,
                "zwnj_mi_prefix": 1,
                "zwnj_plural_ha": 1
            }]}))
        );
    }

    #[test]
    fn literal_sentinel_preserves_every_protected_fragment() {
        let text = "before\0`one` after `two`";
        assert_eq!(
            normalize(json!(text), "keep".into()),
            Ok(json!({"$tuple": [text, {}]}))
        );
    }

    #[test]
    fn suggestion_context_counts_characters_and_excludes_real_words() {
        let source = json!("🐈🐈🐈🐈 میز و زمینِ تر بود و میرود پایان");
        assert_eq!(
            suggestions(source, false).map(|v| json!([
                [&v[0]["type"], &v[0]["context"]],
                [&v[1]["type"], &v[1]["context"]],
                v.as_array().map(Vec::len)
            ])),
            Ok(json!([
                ["maybe_comparative_tar", " میز و زمینِ تر بو"],
                ["maybe_glued_mi", " و میرود پا"],
                2
            ]))
        );
    }

    #[test]
    fn handles_a_long_persian_word_without_exhausting_backtracking() {
        let long_word = "ا".repeat(20_000);
        assert_eq!(
            normalize(json!(format!("خانه ام {long_word}")), "keep".into()),
            Ok(json!({"$tuple": [format!("خانه‌ام {long_word}"), {"zwnj_heh_enclitic": 1}]}))
        );
    }

    #[test]
    fn line_changes_zips_on_lf_without_stripping_cr() {
        assert_eq!(
            line_changes(json!("a\r\nb\nc"), json!("a\nB")),
            Ok(json!([
                {"line": 1, "before": "a\r", "after": "a"},
                {"line": 2, "before": "b", "after": "B"}
            ]))
        );
    }

    #[test]
    fn reports_python_errors_for_non_strings() {
        assert_eq!(
            normalize(json!(false), "keep".into()),
            Err("TypeError: expected string or bytes-like object, got 'bool'".into())
        );
        assert_eq!(
            suggestions(Value::Null, true),
            Err("TypeError: expected string or bytes-like object, got 'NoneType'".into())
        );
        assert_eq!(
            line_changes(json!("a"), json!([])),
            Err("AttributeError: 'list' object has no attribute 'split'".into())
        );
    }
}
