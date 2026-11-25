use super::*;
use serde_json::json;

#[test]
fn ass_floors_both_ends_and_refuses_collapsed_cues() {
    assert_eq!(
        effective_times(19, 39, "ass".into()),
        Ok(json!({"$tuple":[10,30]}))
    );
    assert!(effective_times(10, 19, "ass".into()).is_err());
}

#[test]
fn conversion_keeps_layout_resolutions_and_literal_ass_prose() -> Result<(), String> {
    let source = concat!(
        "[Script Info]\r\nPlayResX: 640\r\nPlayResY: 360\r\nLayoutResX: 320\r\nLayoutResY: 180\r\n",
        "[V4+ Styles]\r\nFormat: Name\r\nStyle: Default\r\n",
        "[Aegisub Project Garbage]\r\nLast Style Storage: Default\r\n",
        "[Events]\r\nFormat: Start, End, Style, Text\r\nDialogue: 0:00:01.00,0:00:02.00,Default,سلام <i>literal</i>"
    );
    let doc = document::parse(source, "ass")?;
    let options = conversion::Options {
        direction: "ltr",
        fix_zwnj: false,
        strip_watermarks: false,
        wrap_lines: false,
        ..conversion::Options::default()
    };
    let (converted, removed) = conversion::ass_document_to_ass(&doc, &options)?;
    assert!(removed.is_empty());
    let rendered = document::serialize(&converted, &converted.cues, false)?;
    assert!(rendered.contains("PlayResX: 640\nPlayResY: 360\nLayoutResX: 320\nLayoutResY: 180"));
    assert!(rendered.contains("سلام <i>literal</i>"));
    assert!(!rendered.contains("[Aegisub Project Garbage]"));
    Ok(())
}
#[test]
fn ass_keeps_original_order_for_quantized_overlaps() -> Result<(), String> {
    let cues = [
        Cue {
            start: 109,
            end: 170,
            ..Cue::default()
        },
        Cue {
            start: 96,
            end: 130,
            ..Cue::default()
        },
        Cue {
            start: 40,
            end: 80,
            ..Cue::default()
        },
    ];
    assert_eq!(document::presentation_order(&cues, "ass")?, vec![2, 0, 1]);
    Ok(())
}
