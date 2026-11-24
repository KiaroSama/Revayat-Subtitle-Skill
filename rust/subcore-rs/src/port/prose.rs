use fancy_regex::Captures;

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

fn normalize_prose(text: &str) -> Result<String, String> {
    let text = letters(text)?;
    let verb = pattern!(
        FancyRegex,
        format!(
            "(?<![{PROSE_WORD}])(ن?می) +((?:{STEMS})(?:یم|ید|ند|یم|ین|ن|م|ی|د|ه)?)(?![{PROSE_WORD}])"
        )
    );
    let followed_by_persian = pattern!(Regex, format!("\\A +{FA}"));
    let text = verb
        .try_replacen(&text, 0, |m: &Captures<'_, str>| {
            if m.get(2).is_some_and(|stem| {
                stem.as_str() == "روی" && followed_by_persian.is_match(&text[stem.end()..])
            }) {
                m[0].to_owned()
            } else {
                format!("{}{ZWNJ}{}", &m[1], &m[2])
            }
        })
        .map_err(regex_error)?;
    let suffix = pattern!(
        FancyRegex,
        format!(
            "(?<![{PROSE_WORD}])({FA}(?:{FA}|{ZWNJ})*) +((?:ها(?:ی(?:مان|تان|شان|م|ت|ش|ی)?)?|ترین|تر))(?![{PROSE_WORD}])"
        )
    );
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
    let text = pattern!(FancyRegex, format!(
        "(?<![{PROSE_WORD}])({FA}(?:{FA}|{ZWNJ})*ه) +(ام|ات|اش|ایم|اید|اند|ای)(?![{PROSE_WORD}])"
    )).try_replacen(&text, 0, format!("${{1}}{ZWNJ}${{2}}")).map_err(regex_error)?;
    let compound = pattern!(
        FancyRegex,
        format!(
            "(?<![{PROSE_WORD}])({})(?![{PROSE_WORD}])",
            COMPOUNDS.map(|(from, _)| from).join("|")
        )
    );
    compound
        .try_replacen(&text, 0, |m: &Captures<'_, str>| {
            COMPOUNDS
                .iter()
                .find(|(from, _)| *from == &m[0])
                .map_or(&m[0], |(_, to)| to)
                .to_owned()
        })
        .map(|s| s.into_owned())
        .map_err(regex_error)
}

pub(super) fn normalize(text: &str) -> Result<String, String> {
    let protected = pattern!(
        FancyRegex,
        format!(
            r"{ASS}|{HTML}|\\[NnHh]|(?:[A-Za-z][A-Za-z0-9+.-]*://|[Ww][Ww][Ww]\.|[Tt]\.[Mm][Ee]/)[^\x09-\x0d <>{{}}\\]+|(?:(?<![A-Za-z0-9_])(?=[A-Za-z0-9])|(?<=[A-Za-z0-9_])(?=-))(?:[A-Za-z0-9-]+\.)+[A-Za-z]{{2,}}/[^\x09-\x0d <>{{}}\\]*|[A-Za-z0-9_.+-]+@[A-Za-z0-9_.-]+\.[A-Za-z]{{2,}}"
        )
    );
    let mut output = String::with_capacity(text.len());
    for segment in segments(text, protected) {
        let (prose, literal) = segment?;
        if !prose.is_empty() {
            output.push_str(&normalize_prose(prose)?);
        }
        output.push_str(literal);
    }
    Ok(output)
}

pub(super) fn watermark(text: &str) -> Result<&'static str, String> {
    let mut plain = String::with_capacity(text.len());
    for segment in segments(text, tags()) {
        let (prose, markup) = segment?;
        plain.push_str(prose);
        if srt_break(markup) {
            plain.push(' ');
        }
    }
    let plain = pattern!(Regex, r"\\[Nnh]").replace_all(&plain, " ");
    let plain = letters(&plain.replace(|c| BIDI.contains(c), "").replace(ZWNJ, " "))?;
    let plain = caseless::default_case_fold_str(&plain);
    let plain = plain
        .split(py_space)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let studio = pattern!(
        FancyRegex,
        format!(
            r"(?<![{WORD}])(?:deluxe media|sous[- ]titres? *:|sous[- ]titrage *:|supervision créative|vertaling *:|creatief supervisor)(?![{WORD}])|日本語字幕"
        )
    );
    let credit = pattern!(
        FancyRegex,
        format!(
            r"(?<![{WORD}])(?:زیرنویس از|ترجمه و تنظیم|تنظیم و ترجمه|مترجم *:|تقدیم به نگاه|sync(?:ed)? and corrected by|subtitles by|captioned by|encoded by|downloaded from|support us and become vip|advertise your product)(?![{WORD}])"
        )
    );
    let site = pattern!(
        FancyRegex,
        format!(
            r"(?<![{WORD}])(?:addic7ed|opensubtitles|subscene|yify|yts|shine021|farsisubtitle|worldsubtitle|subf2m|کانال تلگرام|ساب ?تایتل|فارس ?ساب|موویکو|دیجی ?موویز|زارفیل|سافت ?ساب)(?![{WORD}])"
        )
    );
    let handle = pattern!(Regex, r"\A[\[({]? *@[a-z0-9_]+ *[\])}]?\z");
    let telegram = pattern!(FancyRegex, format!(r"(?<![{WORD}])t\.me/[^ ]+"));
    let link = pattern!(
        FancyRegex,
        format!(r"(?<![{WORD}@])(?:https?://|www\.|t\.me/)[^ ]+")
    );
    if studio.is_match(&plain).map_err(regex_error)? {
        Ok("studio-credit")
    } else if credit.is_match(&plain).map_err(regex_error)? {
        Ok("fansub-credit")
    } else if site.is_match(&plain).map_err(regex_error)?
        || matches!(
            plain.trim_matches(|c| " .!«»[]():-•".contains(c)),
            "اپلیکیشن" | "وب سایت" | "وبسایت"
        )
    {
        Ok("site-watermark")
    } else if handle.is_match(&plain) || telegram.is_match(&plain).map_err(regex_error)? {
        Ok("telegram-handle")
    } else if link.is_match(&plain).map_err(regex_error)? {
        Ok("url-ad")
    } else {
        Ok("")
    }
}
