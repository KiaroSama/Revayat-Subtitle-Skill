use revayat_core::persian_lint;
use serde_json::{Value, json};

fn styles(text: &str) -> Result<Vec<Value>, String> {
    Ok(persian_lint::suggestions(json!(text), true)?
        .as_array()
        .expect("suggestions are an array")
        .iter()
        .filter(|suggestion| suggestion["type"] == "style")
        .cloned()
        .collect())
}

#[test]
fn long_logical_line_retains_real_style_matches() -> Result<(), String> {
    let text = format!("{} در پایان", "x".repeat(200_000));
    let found = styles(&text)?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["hint"], "summary connector; usually delete");
    assert_eq!(found[0]["context"], format!("{} در پایان", "x".repeat(14)));
    Ok(())
}

#[test]
fn long_matching_style_body_does_not_exhaust_a_backtracking_stack() -> Result<(), String> {
    let text = format!("یک {} است", "x".repeat(1_000_000));
    let found = styles(&text)?;
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0]["hint"],
        "«یک X است» often calques English 'a'; Persian usually says «Xی است» or just «X است»"
    );
    assert_eq!(found[0]["context"], text);
    Ok(())
}

const RULE_EXAMPLES: &[(&str, &str)] = &[
    ("می‌باشد", "«می‌باشد»"),
    ("می‌گردند", "«می‌گردد»"),
    ("مورد استفاده قرار می‌گیرد", "«مورد … قرار گرفتن»"),
    ("دارای", "«دارای X است»"),
    ("توسط", "«توسط»"),
    ("به‌عنوان", "«به عنوان»"),
    ("در راستای", "bureaucratic connector"),
    ("شایان ذکر است", "announcement filler"),
    ("نه تنها خواند بلکه نوشت", "«نه تنها … بلکه»"),
    ("در پایان", "summary connector"),
    ("این مسئله", "abstract subject"),
    ("نقش کلیدی", "«نقش مهمی ایفا می‌کند»"),
    ("یک خانه است", "«یک X است»"),
];

#[test]
fn every_style_rule_retains_matching_and_original_rule_order() -> Result<(), String> {
    let prefix = "x".repeat(200_000);
    for &(text, hint) in RULE_EXAMPLES {
        let found = styles(&format!("{prefix} {text}"))?;
        assert_eq!(found.len(), 1, "{text}");
        assert!(found[0]["hint"].as_str().unwrap().starts_with(hint));
    }
    let reversed = RULE_EXAMPLES
        .iter()
        .rev()
        .map(|&(text, _)| text)
        .collect::<Vec<_>>()
        .join(". ");
    let found = styles(&reversed)?;
    assert_eq!(found.len(), RULE_EXAMPLES.len());
    for (result, &(_, hint)) in found.iter().zip(RULE_EXAMPLES) {
        assert!(result["hint"].as_str().unwrap().starts_with(hint));
    }
    Ok(())
}

#[test]
fn style_boundaries_use_letters_numbers_and_underscore_not_alphabetic_marks() -> Result<(), String>
{
    for word in ["A", "ا", "字", "1", "Ⅷ", "²", "_", "\u{1e4d0}"] {
        for text in [format!("{word}دارای"), format!("دارای{word}")] {
            assert!(styles(&text)?.is_empty(), "{text}");
        }
    }
    for separator in [
        "\u{064e}", "\u{0345}", "\u{093e}", "\u{0301}", "\u{200c}", "\u{200d}", "🐈",
    ] {
        let text = format!("{separator}دارای{separator}");
        let found = styles(&text)?;
        assert_eq!(found.len(), 1, "{text}");
        assert_eq!(found[0]["context"], text);
    }
    assert_eq!(styles("a\u{0301}دارای")?.len(), 1);
    assert_eq!(styles("نقش مهمی")?.len(), 1);
    assert!(styles("ایفا می‌کندx")?.is_empty());
    assert_eq!(styles("ایفا می‌کند\u{200c}")?.len(), 1);
    Ok(())
}

#[test]
fn style_whitespace_and_context_preserve_all_supported_line_separators() -> Result<(), String> {
    for separator in [
        "\n", "\r\n", "\r", "\u{0085}", "\u{2028}", "\u{001c}", "\u{001f}", "\t", "\u{00a0}",
    ] {
        let text = format!(
            "🐈{separator}مورد{separator}استفاده{separator}قرار{separator}گرفت{separator}🧪"
        );
        let found = styles(&text)?;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0]["context"], text.replace('\n', " "));
        let text = format!("🐈{separator}یک{separator}خانه{separator}است{separator}🧪");
        let found = styles(&text)?;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0]["context"], text.replace('\n', " "));
    }
    assert!(styles("نه تنها خواند\nبلکه نوشت")?.is_empty());
    assert_eq!(styles("نه تنها 🐈 بلکه نوشت")?.len(), 1);
    assert_eq!(
        styles(&format!("نه تنها {} بلکه", "🐈".repeat(60)))?.len(),
        1
    );
    assert!(styles(&format!("نه تنها {} بلکه", "🐈".repeat(61)))?.is_empty());
    Ok(())
}

#[test]
fn rejected_boundaries_do_not_skip_an_overlapping_later_candidate() -> Result<(), String> {
    let text = "xیک یک است است";
    let found = styles(text)?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["context"], text);
    assert!(styles("یک خانه استی")?.is_empty());
    assert_eq!(styles("یک خانه است\u{064e}")?.len(), 1);
    Ok(())
}

#[test]
fn dense_false_prefixes_do_not_hide_a_real_style_match() -> Result<(), String> {
    let text = format!("{}در نهایت", "xدر نهایت ".repeat(100_000));
    let found = styles(&text)?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["hint"], "summary connector; usually delete");
    Ok(())
}

#[test]
fn style_search_accepts_a_logical_line_at_the_cli_byte_limit() -> Result<(), String> {
    let suffix = " در پایان";
    let mut text = "x".repeat(16 * 1024 * 1024 - suffix.len());
    text.push_str(suffix);
    let found = styles(&text)?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["hint"], "summary connector; usually delete");
    assert_eq!(found[0]["context"], format!("{} در پایان", "x".repeat(14)));
    Ok(())
}

#[test]
fn normalization_percent_scan_is_bounded_and_preserves_exact_replacement_counts()
-> Result<(), String> {
    for text in [
        "x".repeat(1_000_000),
        "ا".repeat(1_000_000),
        "%".repeat(1_000_000),
    ] {
        assert_eq!(
            persian_lint::normalize(json!(text), "keep".into())?,
            json!({"$tuple": [text, {}]})
        );
    }
    for (text, expected, count) in [
        ("۱%۲%", "۱٪۲٪", 2),
        ("۱%%۲%", "۱٪%۲٪", 2),
        ("۱%۲%%۳", "۱٪۲٪%۳", 2),
        ("1% ١% %۱", "1% ١% %۱", 0),
        (
            "`۱%` https://example.test/۲% ۳%",
            "`۱%` https://example.test/۲% ۳٪",
            1,
        ),
    ] {
        let stats = if count == 0 {
            json!({})
        } else {
            json!({"percent_to_persian": count})
        };
        assert_eq!(
            persian_lint::normalize(json!(text), "keep".into())?,
            json!({"$tuple": [expected, stats]})
        );
    }
    Ok(())
}
