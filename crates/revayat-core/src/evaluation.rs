//! Recognizes known rubric wordings without certifying linguistic quality.

#![forbid(unsafe_code)]

use serde_json::{Map, Value, json};
use unicode_normalization::UnicodeNormalization;

const RUBRIC: &str = include_str!("../../../evaluation/cases.json");
const ANSWER_IDS: &str = "ValueError: Answers must map known case IDs to strings";
const ANSWER_TYPE: &str = "ValueError: Every answer must be a string";
const STATES: [&str; 3] = ["known_wording", "needs_review", "missing"];

fn normalized(text: &str) -> String {
    text.trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'))
        .nfc()
        .collect()
}

pub fn score(answers: serde_json::Value) -> Result<serde_json::Value, String> {
    let cases: Vec<Value> = serde_json::from_str(RUBRIC)
        .map_err(|error| format!("ValueError: Invalid evaluation rubric: {error}"))?;
    let answers = answers.as_object().ok_or(ANSWER_IDS)?;
    if answers
        .keys()
        .any(|id| !cases.iter().any(|case| case["id"] == *id))
    {
        return Err(ANSWER_IDS.into());
    }
    if answers.values().any(|answer| !answer.is_string()) {
        return Err(ANSWER_TYPE.into());
    }

    let mut results = Map::new();
    let mut counts = [0; STATES.len()];
    for case in &cases {
        let id = case["id"]
            .as_str()
            .ok_or("ValueError: Invalid rubric case ID")?;
        let accepted = case["accepted"]
            .as_array()
            .ok_or("ValueError: Invalid rubric wordings")?;
        let answer = normalized(answers.get(id).and_then(Value::as_str).unwrap_or_default());
        let state = if answer.is_empty() {
            2
        } else if accepted
            .iter()
            .filter_map(Value::as_str)
            .any(|wording| normalized(wording) == answer)
        {
            0
        } else {
            1
        };
        counts[state] += 1;
        results.insert(id.to_owned(), json!(STATES[state]));
    }
    let counts: Map<String, Value> = STATES
        .into_iter()
        .zip(counts)
        .map(|(state, count)| (state.to_owned(), json!(count)))
        .collect();
    Ok(json!({"results": results, "counts": counts, "linguistic_quality_certified": false}))
}

#[cfg(test)]
mod tests {
    use super::score;
    use serde_json::json;

    #[test]
    fn classifies_known_review_and_missing_without_certifying() -> Result<(), String> {
        let report = score(json!({
            "honorific": "\u{001c}مرسی، رین-سان.\u{00a0}",
            "spoken_persian": "needs review"
        }))?;
        assert_eq!(report["results"]["honorific"], "known_wording");
        assert_eq!(report["results"]["spoken_persian"], "needs_review");
        assert_eq!(report["results"]["mixed_word_order"], "missing");
        assert_eq!(report["counts"]["known_wording"], 1);
        assert_eq!(report["counts"]["needs_review"], 1);
        assert_eq!(report["linguistic_quality_certified"], false);
        Ok(())
    }

    #[test]
    fn rejects_unknown_ids_before_rejecting_non_strings() {
        assert_eq!(
            score(json!({"unexpected": 1})),
            Err("ValueError: Answers must map known case IDs to strings".into())
        );
        assert_eq!(
            score(json!({"honorific": 1})),
            Err("ValueError: Every answer must be a string".into())
        );
    }
}
