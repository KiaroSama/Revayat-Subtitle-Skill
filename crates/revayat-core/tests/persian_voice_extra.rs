use revayat_core::lookup::{dedup, en_pattern, find_en, find_fa, load, lookup_text};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const SKILL: &str = "skills/revayat-subtitle";
const IDIOM_QUERY: &str = "It cost an arm and a leg, so let's break the ice.";

fn bank_path(bank: &str) -> PathBuf {
    Path::new(ROOT)
        .join(SKILL)
        .join("voice/persian/scripts/data")
        .join(bank)
}

fn real_bank(bank: &str) -> Result<Value, String> {
    let path = bank_path(bank)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    load(json!(path))
}

fn english_terms(hits: &Value) -> Result<Vec<&str>, String> {
    hits.as_array()
        .ok_or("Expected a hit list")?
        .iter()
        .map(|hit| {
            hit["$tuple"][0]
                .as_str()
                .ok_or_else(|| format!("Expected an English term: {hit}"))
        })
        .collect()
}

#[test]
fn loads_real_idiom_bank_with_more_than_twenty_rows() -> Result<(), String> {
    let rows = real_bank("idioms.tsv")?;
    assert!(rows.as_array().is_some_and(|bank| bank.len() > 20));
    assert_eq!(load(json!("idioms.tsv"))?, rows);
    Ok(())
}

#[test]
fn loads_real_glossary_with_persian_and_usage_note() -> Result<(), String> {
    let rows = real_bank("glossary.tsv")?;
    let hit = json!({"$tuple": ["business partner", "شریک تجاری", "not «شریک کاری» (P4)"]});
    assert!(rows.as_array().is_some_and(|bank| bank.contains(&hit)));
    assert_eq!(load(json!("glossary.tsv"))?, rows);
    Ok(())
}

#[test]
fn missing_bank_is_empty() -> Result<(), String> {
    assert_eq!(load(json!("nonexistent-t017-test-bank.tsv"))?, json!([]));
    Ok(())
}

#[test]
fn en_pattern_inflects_final_word_and_expands_whitespace() -> Result<(), String> {
    assert_eq!(
        en_pattern(json!("  business\tpartner  "))?,
        json!({"$pattern": [r"\bbusiness\s+partner(?:es|s|ed|ing|d)?\b", 2]})
    );
    Ok(())
}

#[test]
fn en_pattern_respects_acronym_case_and_no_suffix() -> Result<(), String> {
    assert_eq!(
        en_pattern(json!("AI"))?,
        json!({"$pattern": [r"\bAI\b", 0]})
    );
    Ok(())
}

#[test]
fn en_pattern_escapes_literal_regex_metacharacters() -> Result<(), String> {
    assert_eq!(
        en_pattern(json!("a+b"))?,
        json!({"$pattern": [r"\ba\+b(?:es|s|ed|ing|d)?\b", 2]})
    );
    Ok(())
}

#[test]
fn finds_both_idioms_in_forward_prose() -> Result<(), String> {
    let hits = find_en(real_bank("idioms.tsv")?, json!(IDIOM_QUERY))?;
    let mut terms = english_terms(&hits)?;
    terms.sort_unstable();
    assert_eq!(terms, ["break the ice", "cost an arm and a leg"]);
    Ok(())
}

#[test]
fn finds_forward_glossary_plural() -> Result<(), String> {
    let hits = find_en(real_bank("glossary.tsv")?, json!("my business partners"))?;
    assert!(english_terms(&hits)?.contains(&"business partner"));
    Ok(())
}

#[test]
fn does_not_find_ai_inside_aid() -> Result<(), String> {
    let hits = find_en(real_bank("glossary.tsv")?, json!("first aid kit"))?;
    assert!(!english_terms(&hits)?.contains(&"AI"));
    Ok(())
}

#[test]
fn finds_uppercase_acronym_as_a_whole_term() -> Result<(), String> {
    let hits = find_en(real_bank("glossary.tsv")?, json!("AI tools"))?;
    assert!(english_terms(&hits)?.contains(&"AI"));
    Ok(())
}

#[test]
fn lowercase_ai_does_not_match_acronym() -> Result<(), String> {
    let hits = find_en(real_bank("glossary.tsv")?, json!("ai tools"))?;
    assert!(!english_terms(&hits)?.contains(&"AI"));
    Ok(())
}

#[test]
fn unrelated_prose_has_no_idiom_false_positive() -> Result<(), String> {
    let hits = find_en(
        real_bank("idioms.tsv")?,
        json!("The weather is nice today."),
    )?;
    assert_eq!(hits, json!([]));
    Ok(())
}

#[test]
fn finds_glossary_in_reverse() -> Result<(), String> {
    let hits = find_fa(real_bank("glossary.tsv")?, json!("همزمان برق رفت"))?;
    assert!(english_terms(&hits)?.contains(&"the lights went out"));
    Ok(())
}

#[test]
fn reverse_uses_first_persian_idiom_variant() -> Result<(), String> {
    let bank = real_bank("idioms.tsv")?;
    let first = find_fa(bank.clone(), json!("او برای یخ مجلس را شکستن آماده بود."))?;
    assert!(english_terms(&first)?.contains(&"break the ice"));
    let second = find_fa(bank, json!("سر صحبت را باز کردن"))?;
    assert!(!english_terms(&second)?.contains(&"break the ice"));
    Ok(())
}

#[test]
fn dedup_keeps_first_full_bank_row_in_order() -> Result<(), String> {
    let hits = find_en(real_bank("idioms.tsv")?, json!(IDIOM_QUERY))?;
    let rows = hits.as_array().ok_or("Expected an idiom hit array")?;
    assert_eq!(rows.len(), 2);
    let original = rows[0]["$tuple"].as_array().ok_or("Expected a bank row")?;
    let duplicate = json!({"$tuple": [original[0], "درود", "duplicate note"]});
    assert_eq!(dedup(json!([rows[0], rows[1], duplicate, rows[1]]))?, hits);
    Ok(())
}

fn has_entry(bank: &Value, en: &str, fa: &str) -> bool {
    bank.as_array().is_some_and(|entries| {
        entries
            .iter()
            .any(|entry| entry["en"] == en && entry["fa"] == fa && entry["note"].is_string())
    })
}

#[test]
fn lookup_text_returns_both_banks_forward_with_fields() -> Result<(), String> {
    let hits = lookup_text("AI tools help break the ice with business partners.", false)?;
    assert!(has_entry(
        &hits["idioms"],
        "break the ice",
        "یخ مجلس را شکستن / سر صحبت را باز کردن"
    ));
    assert!(has_entry(&hits["glossary"], "AI", "هوش مصنوعی"));
    assert!(has_entry(
        &hits["glossary"],
        "business partner",
        "شریک تجاری"
    ));
    Ok(())
}

#[test]
fn lookup_text_returns_both_banks_reverse_with_fields() -> Result<(), String> {
    let hits = lookup_text("کلی آب خوردن؛ همزمان برق رفت.", true)?;
    assert!(has_entry(
        &hits["idioms"],
        "cost an arm and a leg",
        "کلی آب خوردن / خرج روی دست گذاشتن"
    ));
    assert!(has_entry(
        &hits["glossary"],
        "the lights went out",
        "برق رفت"
    ));
    Ok(())
}

#[test]
fn lookup_text_returns_each_term_once_across_repeated_mentions() -> Result<(), String> {
    let hits = lookup_text("AI AI, break the ice and break the ice again", false)?;
    for (bank, term) in [("glossary", "AI"), ("idioms", "break the ice")] {
        assert_eq!(
            hits[bank]
                .as_array()
                .ok_or("Expected a bank")?
                .iter()
                .filter(|hit| hit["en"] == term)
                .count(),
            1
        );
    }
    Ok(())
}

#[test]
fn lookup_text_returns_empty_banks_for_unmatched_text() -> Result<(), String> {
    assert_eq!(
        lookup_text("The weather is nice today.", false)?,
        json!({"idioms": [], "glossary": []})
    );
    Ok(())
}

const POLICY: &str = include_str!("../../../skills/revayat-subtitle/references/target-voice.md");
const MANIFEST: &str = include_str!("../../../.claude-plugin/plugin.json");
const MAIN_GUIDE: &str = include_str!("../../../skills/revayat-subtitle/SKILL.md");
const PERSIAN: &str = include_str!("../../../skills/revayat-subtitle/voice/persian/SKILL.md");
const HUMANIZER: &str = include_str!("../../../skills/revayat-subtitle/voice/humanizer/SKILL.md");

fn check_guide(name: &str, language: &str, resources: &[&str]) -> Result<(), serde_json::Error> {
    let manifest: Value = serde_json::from_str(MANIFEST)?;
    let route = format!(
        "| `{language}` | [{name}](../voice/{name}/SKILL.md) | `revayat-subtitle:{name}` |"
    );
    assert!(
        POLICY.lines().any(|line| line.starts_with(&route)),
        "{name}"
    );
    assert!(
        manifest["skills"]
            .as_array()
            .is_some_and(|entries| entries.contains(&json!(format!("./{SKILL}/voice/{name}")))),
        "{name}"
    );
    let guide_root = Path::new(ROOT).join(SKILL).join("voice").join(name);
    for resource in ["SKILL.md", "LICENSE"]
        .into_iter()
        .chain(resources.iter().copied())
    {
        let path = guide_root.join(resource);
        assert!(
            path.metadata()
                .is_ok_and(|meta| meta.is_file() && meta.len() > 0),
            "{path:?}"
        );
    }
    Ok(())
}

#[test]
fn main_skill_links_target_voice_policy() {
    assert!(MAIN_GUIDE.contains("(references/target-voice.md)"));
}

#[test]
fn persian_guide_and_banks_are_routed_and_declared() -> Result<(), serde_json::Error> {
    check_guide(
        "persian",
        "fa",
        &[
            "NOTICE",
            "scripts/data/idioms.tsv",
            "scripts/data/glossary.tsv",
            "references/typography-and-rtl.md",
        ],
    )
}

#[test]
fn russian_guide_and_typography_are_routed_and_declared() -> Result<(), serde_json::Error> {
    check_guide("great-russian", "ru", &["references/typography.md"])
}

#[test]
fn english_humanizer_guide_is_routed_and_declared() -> Result<(), serde_json::Error> {
    check_guide("humanizer", "en", &[])
}

#[test]
fn persian_guide_routes_lint_and_lookup_to_rust_cli() {
    assert!(PERSIAN.contains("`revayat-subtitle lint`"));
    assert!(PERSIAN.contains("`revayat-subtitle lookup`"));
    assert!(PERSIAN.contains("revayat-subtitle lookup --reverse"));
    assert!(!PERSIAN.contains("persian_lint.py"));
}

#[test]
fn humanizer_is_bundled_at_version_four() {
    let frontmatter = HUMANIZER.split("---").nth(1).unwrap_or_default();
    assert!(frontmatter.lines().any(|line| line == "name: humanizer"));
    assert!(
        frontmatter
            .lines()
            .any(|line| line.trim() == "version: \"4.0.0\"")
    );
    assert!(HUMANIZER.contains("https://github.com/blader/humanizer/releases/tag/v4.0.0"));
}
