use revayat_core::{evaluation::score, persian_lint};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn normalize(text: &str, digits: &str) -> Result<Value, String> {
    persian_lint::normalize(json!(text), digits.into())
}

fn fixed(text: &str, digits: &str) -> Result<String, String> {
    let result = normalize(text, digits)?;
    result["$tuple"][0]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("Expected a normalized string, got {result}"))
}

macro_rules! fixes {
    ($($name:ident: $input:expr => $expected:expr $(, $digits:literal)?;)*) => {
        $(#[test]
        fn $name() -> Result<(), String> {
            assert_eq!(fixed($input, fixes!(@digits $($digits)?))?, $expected);
            Ok(())
        })*
    };
    (@digits $digits:literal) => { $digits };
    (@digits) => { "persian" };
}

const MESSY: &str =
    r#"او مي رود و كتاب ها روي ميز است ، و بزرگ ترين آرزو همين بود 1964 . گفت "خوب" ; چرا?"#;
const MIXED: &str = "Phase 3: سلام, خوبی? 1. item";

fixes! {
    arabic_yeh_and_kaf: "كتاب و مي" => "کتاب و می";
    arabic_teh_marbuta: "علاقة" => "علاقه";
    arabic_alef_maksura: "موسى" => "موسی";
    preserves_persian_only_letters: "پژوهش گچ" => "پژوهش گچ";
    preserves_tanwin: "مثلاً بعداً" => "مثلاً بعداً";
    removes_kashida_elongation: "کتـــاب" => "کتاب";
    spaced_kashida_becomes_en_dash: "پر می‌کردند ـ بابا" => "پر می‌کردند \u{2013} بابا";
    converts_latin_digits: "سال 1964" => "سال ۱۹۶۴";
    converts_arabic_indic_digits: "سال ١٩٦٤" => "سال ۱۹۶۴";
    preserves_alphanumeric_tokens: "فایل MP3 و COVID-19" => "فایل MP3 و COVID-19";
    preserves_versions: "نسخه v1.2.3 و 2.4.1" => "نسخه v1.2.3 و 2.4.1";
    keeps_thousands_comma: "قیمت 1,000 تومان" => "قیمت ۱,۰۰۰ تومان";
    converts_percent_in_persian_prose: "50% مردم" => "۵۰٪ مردم";
    keep_digit_mode: "نسخه 2 و سال 1990" => "نسخه 2 و سال 1990", "keep";
    latin_digit_mode: "سال ۱۹۶۴" => "سال 1964", "latin";
    preserves_url: "ببین https://x.com/a?b=1,2 را" => "ببین https://x.com/a?b=1,2 را";
    preserves_email: "به ali.r@dtu.dk بنویس" => "به ali.r@dtu.dk بنویس";
    preserves_inline_code: "دستور `print(1, 2)` و ```x = 3```" => "دستور `print(1, 2)` و ```x = 3```";
    converts_comma: "سلام, خوبی" => "سلام، خوبی";
    converts_semicolon_and_question: "رفت; آمد?" => "رفت؛ آمد؟";
    preserves_english_punctuation: "Hello, how are you? Fine; thanks." => "Hello, how are you? Fine; thanks.";
    preserves_english_quotes: r#"He said "hi" to me."# => r#"He said "hi" to me."#;
    converts_persian_straight_quotes: r#"او گفت "سلام" و رفت"# => "او گفت «سلام» و رفت";
    converts_persian_curly_quotes: "او گفت “سلام”" => "او گفت «سلام»";
    converts_quotes_around_latin_in_persian: r#"او گفت "hello" و رفت"# => "او گفت «hello» و رفت";
    removes_space_before_comma: "رفت ، آمد" => "رفت، آمد";
    adds_space_after_comma: "رفت،آمد" => "رفت، آمد";
    removes_spaces_inside_guillemets: "« سلام »" => "«سلام»";
    preserves_adjacent_exclamations: "عالی!! واقعاً؟!" => "عالی!! واقعاً؟!";
    converts_ellipsis: "رفت... و آمد" => "رفت… و آمد";
    joins_mi_and_nemi: "او می رود و نمی خواهم" => "او می‌رود و نمی‌خواهم";
    joined_mi_is_stable: "می‌رود" => "می‌رود";
    mi_before_digit_is_not_joined: "می 2" => "می ۲";
    joins_plural_ha: "کتاب ها و بچه هایش" => "کتاب‌ها و بچه‌هایش";
    leaves_ha_after_latin_token: "PDF ها" => "PDF ها";
    leaves_hay_o_hooy: "های و هوی" => "های و هوی";
    joins_superlative: "بزرگ ترین" => "بزرگ‌ترین";
    comparative_is_not_autocorrected: "بزرگ تر" => "بزرگ تر";
    joins_heh_enclitic: "خانه ام و رفته اند" => "خانه‌ام و رفته‌اند";
    joins_indefinite_heh_enclitic: "به خانه ای رفت" => "به خانه‌ای رفت";
    leaves_heh_stopword: "که ای دوست" => "که ای دوست";
    eval_four_sample: MESSY => "او می‌رود و کتاب‌ها روی میز است، و بزرگ‌ترین آرزو همین بود ۱۹۶۴. گفت «خوب»؛ چرا؟";
    english_line_keeps_digits: "Phase 3 uses P8 and «چرا» twice." => "Phase 3 uses P8 and «چرا» twice.";
    english_list_keeps_commas: "Use ZWNJ in (RTL, نیم‌فاصله, digits)." => "Use ZWNJ in (RTL, نیم‌فاصله, digits).";
    persian_words_flanking_comma: "word سلام, خوبی word word word" => "word سلام، خوبی word word word";
    list_marker_keeps_latin_digit: "1. کتاب ها را بخوان" => "1. کتاب‌ها را بخوان";
    preserves_indentation: "    کد تو رفتگی\n  - آیتم" => "    کد تو رفتگی\n  - آیتم";
    number_only_line_follows_persian_document: "سلام دنیا\n1964\nخداحافظ" => "سلام دنیا\n۱۹۶۴\nخداحافظ";
    number_only_line_follows_english_document: "hello world\n1964\nbye" => "hello world\n1964\nbye";
    english_quote_next_to_persian_stays: r#"- "Isn't 1947 the date?" → «مگر … نیست؟»"# => r#"- "Isn't 1947 the date?" → «مگر … نیست؟»"#;
    persian_majority_quote_on_english_line: r#"the word "کتاب ها" means books"# => "the word «کتاب‌ها» means books";
    english_period_spacing_untouched: "end . next" => "end . next";
    persian_period_spacing_fixed: "رفت . آمد" => "رفت. آمد";
    protects_indented_code: "    python x.py --fix  file.txt   # سلام" => "    python x.py --fix  file.txt   # سلام";
    english_list_of_marks_keeps_spaces: "no space before ، ؛ ؟ in Persian" => "no space before ، ؛ ؟ in Persian";
    persian_phrase_in_english_line_still_fixed: "the phrase «سلام ، خوبی» means hi" => "the phrase «سلام، خوبی» means hi";
    fenced_code_ends_before_prose: "```python\nكتاب ها\nمي رود\n```\n\nكتاب ها مي رود" => "```python\nكتاب ها\nمي رود\n```\n\nکتاب‌ها می‌رود", "keep";
    keeps_persian_digits_in_keep_mode: "سال ۱۹۶۴" => "سال ۱۹۶۴", "keep";
    latin_mode_converts_arabic_indic_digits: "سال ١٩٦٤" => "سال 1964", "latin";
    leaves_question_mark_in_url: "https://example.com/q?x=123&y=2 و سلام, خوبی?" => "https://example.com/q?x=123&y=2 و سلام، خوبی؟";
    leaves_indented_arabic_letters_before_prose: "\tكتاب ها\nكتاب ها" => "\tكتاب ها\nکتاب‌ها";
}

#[test]
fn persists_no_changes_after_normalizing_full_sample() -> Result<(), String> {
    let once = fixed(MESSY, "persian")?;
    assert_eq!(normalize(&once, "persian")?, json!({"$tuple": [once, {}]}));
    Ok(())
}

#[test]
fn persists_no_changes_after_normalizing_mixed_sample() -> Result<(), String> {
    let once = fixed(MIXED, "persian")?;
    assert_eq!(normalize(&once, "persian")?, json!({"$tuple": [once, {}]}));
    Ok(())
}

fn check_indented_code(newline: &str, indent: &str) -> Result<(), String> {
    let text = format!("{indent}code = 'كتاب ها' {newline}{newline}كتاب ها مي رود");
    let expected = format!("{indent}code = 'كتاب ها' {newline}{newline}کتاب‌ها می‌رود");
    assert_eq!(
        normalize(&text, "keep")?,
        json!({"$tuple": [expected, {
            "arabic_to_persian_letters": 2, "zwnj_mi_prefix": 1, "zwnj_plural_ha": 1
        }]})
    );
    assert_eq!(
        normalize(&expected, "keep")?,
        json!({"$tuple": [expected, {}]})
    );
    Ok(())
}

#[test]
fn f06_lf_four_space_indent() -> Result<(), String> {
    check_indented_code("\n", "    ")
}

#[test]
fn f06_lf_tab_indent() -> Result<(), String> {
    check_indented_code("\n", "\t")
}

#[test]
fn f06_crlf_four_space_indent() -> Result<(), String> {
    check_indented_code("\r\n", "    ")
}

#[test]
fn f06_crlf_tab_indent() -> Result<(), String> {
    check_indented_code("\r\n", "\t")
}

macro_rules! protected_nul {
    ($($name:ident: $input:literal;)*) => {
        $(#[test]
        fn $name() -> Result<(), String> {
            assert_eq!(normalize($input, "keep")?, json!({"$tuple": [$input, {}]}));
            Ok(())
        })*
    };
}

protected_nul! {
    f06_nul_before_multiple_spans: "before\0`one` after `two`";
    f06_nul_between_protected_spans: "\0`1`\0`2`\0";
    f06_nul_within_a_protected_span: "`one\0two`\0end";
}

fn suggestions(text: &str, style: bool) -> Result<Vec<Value>, String> {
    let hints = persian_lint::suggestions(json!(text), style)?;
    hints
        .as_array()
        .cloned()
        .ok_or_else(|| format!("Expected suggestion array, got {hints}"))
}

#[test]
fn suggests_comparative_without_rewriting_wet() -> Result<(), String> {
    assert!(
        suggestions("زمینِ تر بود و میرود و میز", false)?
            .iter()
            .any(|hint| hint["type"] == "maybe_comparative_tar")
    );
    Ok(())
}

#[test]
fn flags_glued_mi_once_and_does_not_flag_miz() -> Result<(), String> {
    assert_eq!(
        suggestions("زمینِ تر بود و میرود و میز", false)?
            .iter()
            .filter(|hint| hint["type"] == "maybe_glued_mi")
            .count(),
        1
    );
    Ok(())
}

#[test]
fn flags_em_dash() -> Result<(), String> {
    assert!(
        suggestions("رفت \u{2014} آمد", false)?
            .iter()
            .any(|hint| hint["type"] == "em_dash")
    );
    Ok(())
}

#[test]
fn style_flags_exact_officialese_set() -> Result<(), String> {
    let hints = suggestions("این کتاب دارای سه فصل می‌باشد و توسط او نوشته شد.", true)?;
    let prefixes: BTreeSet<String> = hints
        .iter()
        .filter(|hint| hint["type"] == "style")
        .filter_map(|hint| {
            hint["hint"]
                .as_str()
                .map(|hint| hint.chars().take(6).collect())
        })
        .collect();
    assert_eq!(
        prefixes,
        ["«می‌با", "«دارای", "«توسط»"].map(str::to_owned).into()
    );
    Ok(())
}

#[test]
fn style_flags_are_opt_in() -> Result<(), String> {
    assert!(
        suggestions("می‌باشد", false)?
            .iter()
            .all(|hint| hint["type"] != "style")
    );
    Ok(())
}

#[test]
fn f06_suggestions_continue_after_indented_code() -> Result<(), String> {
    let hints = suggestions("    میرود code\n\nمیرود", false)?;
    assert_eq!(
        hints.iter().map(|hint| &hint["type"]).collect::<Vec<_>>(),
        ["maybe_glued_mi"]
    );
    Ok(())
}

#[test]
fn style_skips_protected_inline_code() -> Result<(), String> {
    let hints = suggestions("`می‌باشد` و دارای", true)?;
    let matching: Vec<_> = hints
        .iter()
        .filter(|hint| hint["type"] == "style")
        .collect();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0]["hint"], "«دارای X است» -> «X دارد»");
    Ok(())
}

#[test]
fn style_flags_stock_contrast_without_changing_text() -> Result<(), String> {
    let input = "نه تنها کتاب خواند بلکه نوشت";
    assert!(suggestions(input, true)?.iter().any(|hint| {
        hint["hint"]
            .as_str()
            .is_some_and(|hint| hint.starts_with("«نه تنها"))
    }));
    assert_eq!(fixed(input, "persian")?, input);
    Ok(())
}

#[test]
fn style_flags_announcement_filler() -> Result<(), String> {
    let hints = suggestions("لازم به ذکر است که کتاب تمام شد.", true)?;
    assert_eq!(
        hints
            .iter()
            .filter(|hint| hint["hint"] == "announcement filler; delete it and state the point")
            .count(),
        1
    );
    Ok(())
}

#[test]
fn style_flags_stock_summary() -> Result<(), String> {
    let hints = suggestions("در مجموع کتاب را خواند.", true)?;
    assert_eq!(
        hints
            .iter()
            .filter(|hint| hint["hint"] == "summary connector; usually delete")
            .count(),
        1
    );
    Ok(())
}

#[test]
fn change_report_skips_protected_lines_and_uses_one_based_indices() -> Result<(), String> {
    let input = "    كتاب ها\nكتاب ها\nسلام";
    let output = fixed(input, "persian")?;
    assert_eq!(
        persian_lint::line_changes(json!(input), json!(output))?,
        json!([{"line": 2, "before": "كتاب ها", "after": "کتاب‌ها"}])
    );
    Ok(())
}

const CASES: &str = include_str!("../../../evaluation/cases.json");
const PILOT: &str = include_str!("../../../evaluation/pilot.json");

fn cases() -> Result<Vec<Value>, serde_json::Error> {
    serde_json::from_str(CASES)
}

fn pilot() -> Result<Value, serde_json::Error> {
    serde_json::from_str(PILOT)
}

#[test]
fn evaluation_catalogue_has_24_distinct_cases() -> Result<(), serde_json::Error> {
    let cases = cases()?;
    assert_eq!(cases.len(), 24);
    let ids: BTreeSet<_> = cases.iter().map(|case| case["id"].as_str()).collect();
    assert_eq!(ids.len(), 24);
    assert!(!ids.contains(&None));
    Ok(())
}

#[test]
fn pilot_discloses_ai_review_and_refuses_linguistic_certification() -> Result<(), serde_json::Error>
{
    let pilot = pilot()?;
    assert_eq!(pilot["version"], 1);
    assert_eq!(pilot["linguistic_quality_certified"], false);
    assert_eq!(pilot["cases"].as_array().map(Vec::len), Some(5));
    assert!(
        pilot["independent_review"]["role"]
            .as_str()
            .is_some_and(|role| role.contains("AI"))
    );
    assert!(
        pilot["independent_review"]["limitations"]
            .as_str()
            .is_some_and(|text| text.to_lowercase().contains("not independent human"))
    );
    Ok(())
}

#[test]
fn pilot_review_tracks_all_five_ids_and_preserves_outcomes() -> Result<(), serde_json::Error> {
    let pilot = pilot()?;
    let outcomes = |rows: &Value| {
        rows.as_array()
            .into_iter()
            .flatten()
            .map(|case| {
                (
                    case["id"].as_str().map(str::to_owned),
                    case["outcome"].clone(),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let author = outcomes(&pilot["cases"]);
    let review = outcomes(&pilot["independent_review"]["cases"]);
    assert_eq!(
        author.keys().collect::<Vec<_>>(),
        review.keys().collect::<Vec<_>>()
    );
    assert_eq!(review.len(), 5);
    assert!(!review.contains_key(&None));
    for (id, outcome) in [
        ("participant_reversal", "meaning_changed"),
        ("negative_request", "meaning_changed"),
        ("missing_referent", "needs_context"),
        ("faithful_unseen", "needs_context"),
    ] {
        assert_eq!(review[&Some(id.to_owned())], outcome);
    }
    assert_eq!(
        pilot["cases"][0]["author_observation"]["outcome"],
        "faithful"
    );
    Ok(())
}

#[test]
fn scorer_accepts_every_catalogue_wording_without_certifying()
-> Result<(), Box<dyn std::error::Error>> {
    let catalogue = cases()?;
    let max = catalogue
        .iter()
        .filter_map(|case| case["accepted"].as_array().map(Vec::len))
        .max()
        .ok_or("Empty catalogue")?;
    for variant in 0..max {
        let mut answers = serde_json::Map::new();
        for case in &catalogue {
            let id = case["id"].as_str().ok_or("Missing catalogue case ID")?;
            let accepted = case["accepted"]
                .as_array()
                .ok_or("Missing accepted array")?;
            assert!(!accepted.is_empty(), "{id}");
            answers.insert(
                id.to_owned(),
                accepted[variant.min(accepted.len() - 1)].clone(),
            );
        }
        let result = score(json!(answers))?;
        assert_eq!(
            result["counts"],
            json!({"known_wording": 24, "needs_review": 0, "missing": 0})
        );
        assert_eq!(result["linguistic_quality_certified"], false);
        for id in answers.keys() {
            assert_eq!(result["results"][id], "known_wording", "{id}");
        }
    }
    Ok(())
}

#[test]
fn pilot_proposals_do_not_become_certified_catalogue_wordings()
-> Result<(), Box<dyn std::error::Error>> {
    let pilot = pilot()?;
    let mut answers = serde_json::Map::new();
    let proposals = pilot["cases"].as_array().ok_or("Missing pilot proposals")?;
    for (pilot_id, catalogue_id) in [
        ("faithful_unseen", "spoken_persian"),
        ("participant_reversal", "zh_passive"),
        ("negative_request", "ja_negative_request"),
        ("missing_referent", "ja_omitted_subject"),
        ("honorific_quote_scope", "honorific"),
    ] {
        let proposal = proposals
            .iter()
            .find(|case| case["id"] == pilot_id)
            .ok_or_else(|| format!("Missing pilot proposal: {pilot_id}"))?;
        answers.insert(catalogue_id.into(), proposal["answer"].clone());
    }
    let result = score(json!(answers))?;
    assert_eq!(
        result["counts"],
        json!({"known_wording": 0, "needs_review": 5, "missing": 19})
    );
    for id in answers.keys() {
        assert_eq!(result["results"][id], "needs_review", "{id}");
    }
    assert_eq!(result["linguistic_quality_certified"], false);
    Ok(())
}

#[test]
fn scorer_refuses_pilot_document_as_answers() -> Result<(), serde_json::Error> {
    assert_eq!(
        score(pilot()?),
        Err("ValueError: Answers must map known case IDs to strings".into())
    );
    Ok(())
}

#[test]
fn scorer_refuses_synthetic_certification_claim() {
    assert_eq!(
        score(json!({"spoken_persian": "می‌خوام برم خونه.", "linguistic_quality_certified": true})),
        Err("ValueError: Answers must map known case IDs to strings".into())
    );
}

#[test]
fn scorer_refuses_non_object_answers() {
    assert_eq!(
        score(json!(["honorific"])),
        Err("ValueError: Answers must map known case IDs to strings".into())
    );
}

#[test]
fn scorer_refuses_non_string_answers_to_known_ids() {
    assert_eq!(
        score(json!({"honorific": true})),
        Err("ValueError: Every answer must be a string".into())
    );
}

#[test]
fn scorer_refuses_unknown_synthetic_ids_before_value_validation() {
    assert_eq!(
        score(json!({"made_up": 1, "honorific": false})),
        Err("ValueError: Answers must map known case IDs to strings".into())
    );
}

#[test]
fn scorer_treats_omitted_and_whitespace_only_answers_as_missing() -> Result<(), String> {
    let result = score(json!({"honorific": " \t\u{1f}\u{2003}"}))?;
    assert_eq!(
        result["counts"],
        json!({"known_wording": 0, "needs_review": 0, "missing": 24})
    );
    assert_eq!(result["results"]["honorific"], "missing");
    assert_eq!(result["results"]["spoken_persian"], "missing");
    assert_eq!(result["linguistic_quality_certified"], false);
    Ok(())
}

#[test]
fn scorer_trims_outer_unicode_space_but_requires_exact_spoken_wording() -> Result<(), String> {
    let result = score(
        json!({"spoken_persian": "\u{2003}می‌خوام برم خونه.\u{1c}", "ja_negative_request": "بهش دست بزن."}),
    )?;
    assert_eq!(
        result["counts"],
        json!({"known_wording": 1, "needs_review": 1, "missing": 22})
    );
    assert_eq!(result["results"]["spoken_persian"], "known_wording");
    assert_eq!(result["results"]["ja_negative_request"], "needs_review");
    assert_eq!(result["linguistic_quality_certified"], false);
    Ok(())
}
