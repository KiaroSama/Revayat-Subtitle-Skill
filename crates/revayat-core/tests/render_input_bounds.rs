use revayat_core::{
    render::{sample_plan, sampling},
    subtitle_formats::document::{Cue, Document, parse},
    validation::MAX_TIME_MS,
};
use serde_json::json;

#[test]
fn public_sampling_rejects_invalid_timing_without_panicking_or_wrapping() {
    for (start, end) in [
        (i64::MIN, 1),
        (-1, 1000),
        (1000, 1000),
        (2000, 1000),
        (0, i64::MAX),
        (MAX_TIME_MS, MAX_TIME_MS + 1),
    ] {
        let doc = Document {
            kind: "srt".into(),
            cues: vec![Cue {
                id: "c000001".into(),
                start,
                end,
                text: "Authored fixture".into(),
                ..Cue::default()
            }],
            ..Document::default()
        };
        assert!(sampling::samples(&doc, false, &[]).is_err());
        assert!(sample_plan(json!({"$object": "Document", "attrs": doc}), true, None).is_err());
    }
}

#[test]
fn sampling_valid_source_precision_and_shared_frames_are_unchanged() -> Result<(), String> {
    let doc = parse(
        "1\n00:00:01,001 --> 00:00:02,009\nFirst\n\n2\n00:00:01,201 --> 00:00:01,809\nSecond\n",
        "srt",
    )?;
    assert_eq!(
        sampling::samples(&doc, true, &[])?,
        [json!({"time_seconds": 1.505, "cue_indices": [1, 2]})]
    );
    Ok(())
}
