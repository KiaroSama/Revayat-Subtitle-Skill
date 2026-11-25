//! Non-backtracking style matching with the linter's exact Unicode word boundaries.

use regex::Regex;
use serde_json::{Value, json};
use std::sync::OnceLock;

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

fn body(pattern: &str) -> Result<Regex, regex::Error> {
    let mut parts = pattern.split(r"\b");
    let mut body = parts.next().unwrap_or_default().to_owned();
    for (index, part) in parts.enumerate() {
        body.push_str(&format!("(?P<boundary{index}>)"));
        body.push_str(part);
    }
    // Preserve the same Python-compatible whitespace as the non-style rules.
    const SPACE: &str = r"\p{White_Space}\x1c-\x1f";
    Regex::new(
        &body
            .replace(r"[^\s", &format!("[^{SPACE}"))
            .replace(r"\s", &format!("[{SPACE}]"))
            .replace(r"\S", &format!("[^{SPACE}]")),
    )
}

fn word_character(word: &Regex, character: Option<char>) -> bool {
    character.is_some_and(|character| {
        let mut buffer = [0; 4];
        word.is_match(character.encode_utf8(&mut buffer))
    })
}

fn boundary(text: &str, position: usize, word: &Regex) -> bool {
    word_character(word, text[..position].chars().next_back())
        != word_character(word, text[position..].chars().next())
}

pub(super) fn suggestions(text: &str) -> Result<Vec<Value>, String> {
    static PATTERNS: OnceLock<Result<(Vec<Regex>, Regex), regex::Error>> = OnceLock::new();
    let (patterns, word) = PATTERNS
        .get_or_init(|| {
            Ok((
                STYLE_FLAGS
                    .iter()
                    .map(|(pattern, _)| body(pattern))
                    .collect::<Result<_, _>>()?,
                Regex::new(r"\A[\p{L}\p{N}_]\z")?,
            ))
        })
        .as_ref()
        .map_err(|error| format!("RuntimeError: {error}"))?;
    let mut output = Vec::new();
    for (pattern, (_, hint)) in patterns.iter().zip(STYLE_FLAGS) {
        let mut cursor = 0;
        while let Some(captures) = pattern.captures_at(text, cursor) {
            let found = captures.get(0).expect("a successful match has a full span");
            // The fixed style alternatives have distinct literal bodies. Their
            // empty captures retain each original boundary, including branches
            // with only a leading boundary. No consuming capture is a boundary.
            if captures
                .iter()
                .skip(1)
                .flatten()
                .all(|capture| boundary(text, capture.start(), word))
            {
                output.push(json!({
                    "type": "style", "hint": hint,
                    "context": super::context_at(text, found.start(), found.end(), 15, 15)
                        .replace('\n', " ")
                }));
                cursor = found.end();
            } else {
                // A rejected outer phrase may contain a later valid candidate.
                // Resume at the next scalar, rather than skipping its full span.
                let Some(character) = text[found.start()..].chars().next() else {
                    break;
                };
                cursor = found.start() + character.len_utf8();
            }
        }
    }
    Ok(output)
}
