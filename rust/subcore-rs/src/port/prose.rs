use fancy_regex::{Captures, RegexInput};
use std::borrow::Cow;

use super::{
    ASS, BIDI, FancyRegex, HTML, Regex, WORD, ZWNJ, py_space, regex_error, segments, srt_break,
    tags,
};

const FA: &str = "[ء-غف-يٮ-ۓە]";
const PROSE_WORD: &str = r"\p{L}\p{N}_\u{200c}";
const STEMS: &str = concat!(
    "رو|روند|آی|آو|آور|بر|بد|باش|شو|ش|کن|تون|توان|دان|دون|گو|گ|گیر|نگیر|بین|شن|شنو|خند|گری|گریه|خور|نوش|",
    "خواه|خواهم|خوای|خورد|خواب|خوان|خون|کش|فروش|خر|گذار|ذار|ساز|سوز|مان|مون|مر|میر|دار|ده|د|دونیم|",
    "زن|زد|افت|ایست|ایس|نشین|شین|پر|ریز|پرس|فهم|فهمید|ترس|رس|رون|ران|تاب|باز|دهش|رقص|جنگ|چرخ|پیچ|",
    "بخش|پذیر|فرست|جوی|کوش|جوش|سپار|شمار|شمارش|لرز|یاب|بند|چسب|گشای|پوش|مال|بوس|لیس|تراش|کار|",
    "رفت|آمد|شد|کرد|گفت|دید|زد|داد|برد|گرفت|داشت|گذاشت|نوشت|نویس|نشد|گشت|بود|خورد|شنید|آورد|پاش|پاشید"
);
const FUNCTION_WORDS: &str =
    "و یا که تا از در بر به با را هم نه اما ولی من تو او ما شما ایشان آنها اینها خود";
const COMPOUNDS: [(&str, &str); 5] = [
    ("هم چنین", "همچنین"),
    ("بی شک", "بی‌شک"),
    ("هیچ وقت", "هیچ‌وقت"),
    ("یک دیگر", "یکدیگر"),
    ("به جای", "به‌جای"),
];

fn letters(text: &str) -> Result<String, String> {
    let translated = text.replace(['ي', 'ى'], "ی").replace('ك', "ک");
    pattern!(FancyRegex, format!("(?<={FA})ة|ة(?={FA})"))
        .try_replacen(&translated, 0, "ه")
        .map(|s| s.into_owned())
        .map_err(regex_error)
}

fn word_pattern(
    template: &'static FancyRegex,
    runtime_word_class: Option<&str>,
) -> Result<Cow<'static, FancyRegex>, String> {
    match runtime_word_class {
        Some(word) => FancyRegex::new(&template.as_str().replace(WORD, word))
            .map(Cow::Owned)
            .map_err(regex_error),
        None => Ok(Cow::Borrowed(template)),
    }
}

fn normalizer_patterns(
    runtime_word_class: Option<&str>,
) -> Result<[Cow<'static, FancyRegex>; 4], String> {
    let verb = pattern!(
        FancyRegex,
        format!(
            "(?<![{PROSE_WORD}])(ن?می) +((?:{STEMS})(?:یم|ید|ند|یم|ین|ن|م|ی|د|ه)?)(?![{PROSE_WORD}])"
        )
    );
    let verb = word_pattern(verb, runtime_word_class)?;
    let suffix = pattern!(
        FancyRegex,
        format!(
            "(?<![{PROSE_WORD}])({FA}(?:{FA}|{ZWNJ})*) +((?:ها(?:ی(?:مان|تان|شان|م|ت|ش|ی)?)?|ترین|تر))(?![{PROSE_WORD}])"
        )
    );
    let suffix = word_pattern(suffix, runtime_word_class)?;
    let enclitic = pattern!(
        FancyRegex,
        format!(
            "(?<![{PROSE_WORD}])({FA}(?:{FA}|{ZWNJ})*ه) +(ام|ات|اش|ایم|اید|اند|ای)(?![{PROSE_WORD}])"
        )
    );
    let enclitic = word_pattern(enclitic, runtime_word_class)?;
    let compound = pattern!(
        FancyRegex,
        format!(
            "(?<![{PROSE_WORD}])({})(?![{PROSE_WORD}])",
            COMPOUNDS.map(|(from, _)| from).join("|")
        )
    );
    let compound = word_pattern(compound, runtime_word_class)?;
    Ok([verb, suffix, enclitic, compound])
}

fn replace_candidates(
    text: &str,
    starts: &Regex,
    pattern: &FancyRegex,
    replace: impl Fn(&Captures<'_, str>) -> String,
) -> Result<String, String> {
    let (mut output, mut cursor) = (String::with_capacity(text.len()), 0);
    // Find possible starts with the linear matcher. Anchoring the full rule at
    // each candidate preserves context without spending VM budget on unrelated prose.
    for start in starts.find_iter(text) {
        if start.start() < cursor {
            continue;
        }
        let input = RegexInput::new(text).from_pos(start.start()).anchored(true);
        if let Some(captures) = pattern.captures_input(input).map_err(regex_error)? {
            let matched = captures.get(0).expect("normalization rules consume text");
            output.push_str(&text[cursor..matched.start()]);
            output.push_str(&replace(&captures));
            cursor = matched.end();
        }
    }
    output.push_str(&text[cursor..]);
    Ok(output)
}

fn normalize_prose(
    text: &str,
    [verb, suffix, enclitic, compound]: &[Cow<'static, FancyRegex>; 4],
) -> Result<String, String> {
    let text = letters(text)?;
    let followed_by_persian = pattern!(Regex, format!("\\A +{FA}"));
    let text = replace_candidates(&text, pattern!(Regex, "[نم]"), verb, |m| {
        if m.get(2).is_some_and(|stem| {
            stem.as_str() == "روی" && followed_by_persian.is_match(&text[stem.end()..])
        }) {
            m[0].to_owned()
        } else {
            format!("{}{ZWNJ}{}", &m[1], &m[2])
        }
    })?;
    let text = suffix
        .try_replacen(&text, 0, |m: &Captures<'_, str>| {
            let (word, suffix) = (&m[1], &m[2]);
            if FUNCTION_WORDS.split_ascii_whitespace().any(|w| w == word)
                || (suffix.starts_with("تر") && word.chars().count() < 2)
            {
                m[0].to_owned()
            } else {
                format!("{word}{ZWNJ}{suffix}")
            }
        })
        .map_err(regex_error)?;
    let text = enclitic
        .try_replacen(&text, 0, format!("${{1}}{ZWNJ}${{2}}"))
        .map_err(regex_error)?;
    let starts = pattern!(
        Regex,
        format!(
            "[{}]",
            COMPOUNDS
                .map(|(from, _)| from.chars().next().unwrap())
                .iter()
                .collect::<String>()
        )
    );
    replace_candidates(&text, starts, compound, |m| {
        COMPOUNDS
            .iter()
            .find(|(from, _)| *from == &m[0])
            .map_or(&m[0], |(_, to)| to)
            .to_owned()
    })
}

pub(super) fn normalize(text: &str, runtime_word_class: Option<&str>) -> Result<String, String> {
    let protected = pattern!(
        FancyRegex,
        format!(
            r"{ASS}|{HTML}|\\[NnHh]|(?:[A-Za-z][A-Za-z0-9+.-]*://|[Ww][Ww][Ww]\.|[Tt]\.[Mm][Ee]/)[^\x09-\x0d <>{{}}\\]+|(?:(?<![A-Za-z0-9_])(?=[A-Za-z0-9])|(?<=[A-Za-z0-9_])(?=-))(?:[A-Za-z0-9-]+\.)+[A-Za-z]{{2,}}/[^\x09-\x0d <>{{}}\\]*|[A-Za-z0-9_.+-]+@[A-Za-z0-9_.-]+\.[A-Za-z]{{2,}}|<!--"
        )
    );
    let patterns = normalizer_patterns(runtime_word_class)?;
    let mut output = String::with_capacity(text.len());
    for segment in segments(text, protected) {
        let (prose, literal) = segment?;
        if !prose.is_empty() {
            output.push_str(&normalize_prose(prose, &patterns)?);
        }
        output.push_str(literal);
    }
    Ok(output)
}

fn credit_line(line: &str, pattern: &FancyRegex, attribution: &Regex) -> Result<bool, String> {
    let Some(found) = pattern.find(line).map_err(regex_error)? else {
        return Ok(false);
    };
    let suffix = &line[found.end()..];
    if !suffix.is_empty() && !suffix.starts_with([' ', ':']) {
        return Ok(false);
    }
    let suffix = suffix.trim_matches([' ', ':']);
    let link = pattern!(Regex, r"\A(?:https?://|www\.|t\.me/)[^ ]+\z");
    Ok(suffix.is_empty() || link.is_match(suffix) || attribution.is_match(suffix))
}

pub(super) fn watermark(text: &str) -> Result<&'static str, String> {
    watermark_prepared(&watermark_text(text)?, None)
}

pub(super) fn watermark_text(text: &str) -> Result<String, String> {
    let mut plain = String::with_capacity(text.len());
    for segment in segments(text, tags()) {
        let (prose, markup) = segment?;
        plain.push_str(prose);
        if srt_break(markup) {
            plain.push('\n');
        }
    }
    let plain = pattern!(Regex, r"\\[Nn]").replace_all(&plain, "\n");
    let plain = plain.replace(r"\h", " ");
    let plain = letters(&plain.replace(|c| BIDI.contains(c), "").replace(ZWNJ, " "))?;
    Ok(caseless::default_case_fold_str(&plain))
}

pub(super) fn watermark_prepared(
    plain: &str,
    runtime_word_class: Option<&str>,
) -> Result<&'static str, String> {
    let studio = pattern!(
        FancyRegex,
        format!(
            r"\A(?:(?:deluxe media|sous[- ]titres? *:|sous[- ]titrage *:|supervision créative|vertaling *:|creatief supervisor)(?![{WORD}])|日本語字幕)"
        )
    );
    let studio = word_pattern(studio, runtime_word_class)?;
    let credit = pattern!(
        FancyRegex,
        format!(
            r"\A(?:زیرنویس از|ترجمه و تنظیم|تنظیم و ترجمه|مترجم *:|تقدیم به نگاه|sync(?:ed)? and corrected by|subtitles by|captioned by|encoded by|downloaded from|support us and become vip|advertise your product)(?![{WORD}])"
        )
    );
    let credit = word_pattern(credit, runtime_word_class)?;
    let site = pattern!(
        FancyRegex,
        format!(
            r"\A(?:addic7ed|opensubtitles|subscene|yify|yts|shine021|farsisubtitle|worldsubtitle|subf2m|کانال تلگرام|ساب ?تایتل|فارس ?ساب|موویکو|دیجی ?موویز|زارفیل|سافت ?ساب)(?![{WORD}])(?:\.[a-z]{{2,}}(?:/\S*)?)?\z"
        )
    );
    let site = word_pattern(site, runtime_word_class)?;
    let attribution = pattern!(
        Regex,
        r"\A[\p{L}\p{N}_@]+(?:[.'’+-][\p{L}\p{N}_@]+)*\.?(?: +[\p{L}\p{N}_@]+(?:[.'’+-][\p{L}\p{N}_@]+)*\.?){0,2}\z"
    );
    let attribution = match runtime_word_class {
        Some(word) => Cow::Owned(
            Regex::new(&attribution.as_str().replace(WORD, word))
                .map_err(|error| format!("RuntimeError: subtitle regex failed: {error}"))?,
        ),
        None => Cow::Borrowed(attribution),
    };
    let handle = pattern!(
        Regex,
        r"\A(?:[({]? *@[a-z0-9_]+ *[)}]?|(?:https?://)?t\.me/\S+)\z"
    );
    let link = pattern!(Regex, r"\A(?:https?://|www\.|t\.me/)[^ ]+\z");
    let mut reason = 5;
    for line in plain.split(|c| {
        matches!(
            c,
            '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}'
                ..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        )
    }) {
        let line = line
            .split(py_space)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let line = line.trim_matches(|c| " .!«»[]():-•".contains(c));
        if line.is_empty() {
            continue;
        }
        let current = if credit_line(line, &studio, &attribution)?
            && (!line.starts_with("deluxe media") || line == "deluxe media")
        {
            0
        } else if credit_line(line, &credit, &attribution)? {
            1
        } else if site.is_match(line).map_err(regex_error)?
            || matches!(line, "اپلیکیشن" | "وب سایت" | "وبسایت")
        {
            2
        } else if handle.is_match(line) {
            3
        } else if link.is_match(line) {
            4
        } else {
            // A mixed dialogue/advertising cue needs an editorial decision.
            return Ok("");
        };
        reason = reason.min(current);
    }
    Ok([
        "studio-credit",
        "fansub-credit",
        "site-watermark",
        "telegram-handle",
        "url-ad",
        "",
    ][reason])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_literal_prefix_preserves_bytes_and_normalizes_persian_tail() {
        let prefix = "<!--".repeat(100_000);
        let source = format!("{prefix}مي روم كتاب ها خانه ام هم چنین");
        let actual =
            normalize(&source, None).expect("long literal prefix must not exhaust regex budget");
        assert_eq!(actual, format!("{prefix}می‌روم کتاب‌ها خانه‌ام همچنین"));
    }
}
