use super::*;
use serde_json::json;

#[test]
fn validates_reviewed_frame_without_echoing_note() -> Result<(), String> {
    let mut frame_data = json!({"file":"frame.png","sha256":"a".repeat(64),"time_seconds":1.5,"width":720,"height":480,"cue_indices":[1,2],"reviewed":true,"note":"private"});
    assert_eq!(frame(frame_data.clone(), "frame".into(), true)?, frame_data);
    frame_data["cue_indices"] = json!([1, 1]);
    assert_eq!(
        frame(frame_data, "frame".into(), true),
        Err("ValueError: frame.cue_indices: duplicate cue index".into())
    );
    Ok(())
}
