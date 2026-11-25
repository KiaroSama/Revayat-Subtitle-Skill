use super::*;

fn valid_report() -> Value {
    json!({
        "audio_duration_ms": 1000,
        "speech_segments_count": 1,
        "global_offset_ms": 0,
        "fps_ratio": 1.0,
        "split_segments": [{"start_cue": 1, "end_cue": 1, "shift_ms": 0}],
        "cues_total": 1,
        "cues_snapped": 0,
        "overlap_score_before": 0.7,
        "overlap_score_after": 0.8,
        "max_cue_delta_ms": 0,
        "median_cue_delta_ms": 0.0
    })
}

#[test]
fn invalid_worker_report_values_are_refused() {
    for (field, value) in [
        ("cues_total", json!(-1)),
        ("cues_total", json!(0)),
        ("cues_total", json!(1.5)),
        ("cues_snapped", json!(2)),
        ("audio_duration_ms", json!(0)),
        ("audio_duration_ms", json!(u64::MAX)),
        (
            "audio_duration_ms",
            json!(subsync_rs::align::MAX_TIMELINE_FRAMES as u64 * 10 + 1),
        ),
        ("speech_segments_count", json!(0)),
        ("speech_segments_count", json!(101)),
        ("speech_segments_count", json!(usize::MAX)),
        ("fps_ratio", json!(-0.5)),
        ("overlap_score_before", json!(-1.0)),
        ("overlap_score_after", json!(2.0)),
        ("max_cue_delta_ms", json!(-1)),
        ("median_cue_delta_ms", json!(-1.0)),
        ("median_cue_delta_ms", json!(1.0)),
        ("split_segments", json!([])),
        ("split_segments", json!([{"unexpected": true}])),
        (
            "split_segments",
            json!([{"start_cue": 0, "end_cue": 1, "shift_ms": 0}]),
        ),
        (
            "split_segments",
            json!([{"start_cue": 2, "end_cue": 1, "shift_ms": 0}]),
        ),
        (
            "split_segments",
            json!([{"start_cue": 1, "end_cue": 2, "shift_ms": 0}]),
        ),
        (
            "split_segments",
            json!([
                {"start_cue": 1, "end_cue": 1, "shift_ms": 0},
                {"start_cue": 1, "end_cue": 1, "shift_ms": 0}
            ]),
        ),
    ] {
        let mut data = valid_report();
        data[field] = value;
        assert!(
            parse_report(&data.to_string()).is_err(),
            "accepted invalid {field}"
        );
    }
}

#[test]
fn report_timeline_and_segment_upper_limits_are_inclusive() {
    let mut value = valid_report();
    value["audio_duration_ms"] = json!(subsync_rs::align::MAX_TIMELINE_FRAMES as u64 * 10);
    value["speech_segments_count"] = json!(subsync_rs::align::MAX_TIMELINE_FRAMES);
    assert!(parse_report(&value.to_string()).is_ok());
    value["speech_segments_count"] = json!(subsync_rs::align::MAX_TIMELINE_FRAMES + 1);
    assert!(parse_report(&value.to_string()).is_err());
}

fn timing_report(cues: usize, shift: u64) -> SyncReport {
    let mut value = valid_report();
    value["cues_total"] = json!(cues);
    value["split_segments"][0]["end_cue"] = json!(cues);
    value["global_offset_ms"] = json!(shift);
    value["split_segments"][0]["shift_ms"] = json!(shift);
    value["max_cue_delta_ms"] = json!(shift);
    value["median_cue_delta_ms"] = json!(shift as f64);
    parse_report(&value.to_string()).unwrap().1
}

#[test]
fn accepts_compatible_reports_without_optional_paths_and_preserves_extra_metadata() {
    let mut value = valid_report();
    value["helper_version"] = json!("compatible previous release");
    let (data, report) = parse_report(&format!("\u{feff}{value}")).unwrap();
    assert_eq!(data, value);
    assert_eq!(report.subtitle_in, None);
}

#[test]
fn timing_only_srt_preserves_bom_multiline_text_tags_and_mixed_separators() {
    let text = "\u{feff}7\r\n00:00:00,100 --> 00:00:00,700 X1:40\n<i>سلام</i>\rsecond line\r\n";
    let original = SubtitleDocument::parse(text, None).unwrap();
    let output = text.replace("00,100", "00,200").replace("00,700", "00,800");
    validate_worker_output(
        output.as_bytes(),
        &original,
        &timing_report(1, 100),
        &SyncConfig::default(),
    )
    .unwrap();
}

#[test]
fn timing_only_ass_keeps_event_order_overlap_reversed_columns_and_attachments() {
    let text = "[Script Info]\rTitle: Authored\r[Events]\rFormat: End, Text, Start\r\
                Dialogue: 0:00:00.90,{\\i1}later, comma,0:00:00.30\r\
                Comment: 0:00:00.90,untouched,0:00:00.30\r\
                Dialogue: 0:00:00.70,earlier,0:00:00.10\r[Fonts]\rfontname: authored\rabc\r";
    let output = text
        .replace(
            "Dialogue: 0:00:00.90,{\\i1}later, comma,0:00:00.30",
            "Dialogue: 0:00:01.00,{\\i1}later, comma,0:00:00.40",
        )
        .replace(
            "Dialogue: 0:00:00.70,earlier,0:00:00.10",
            "Dialogue: 0:00:00.80,earlier,0:00:00.20",
        );
    let original = SubtitleDocument::parse(text, None).unwrap();
    validate_worker_output(
        output.as_bytes(),
        &original,
        &timing_report(2, 100),
        &SyncConfig::default(),
    )
    .unwrap();
    for change in [
        text.replace("authored", "removed"),
        text.replace("{\\i1}", ""),
        text.replace("untouched", "changed"),
    ] {
        assert!(
            validate_worker_output(
                change.as_bytes(),
                &original,
                &timing_report(2, 0),
                &SyncConfig::default()
            )
            .is_err()
        );
    }
}

#[test]
fn refuses_invalid_encoding_grammar_content_and_cue_counts() {
    let text = "1\n00:00:00,100 --> 00:00:00,700\n<i>Authored</i>\n";
    let original = SubtitleDocument::parse(text, None).unwrap();
    for output in [
        vec![0xff, 0xfe],
        b"not a subtitle".to_vec(),
        Vec::new(),
        text.replace("Authored", "Unrelated").into_bytes(),
        text.replacen("1\n", "8\n", 1).into_bytes(),
        text.replace('\n', "\r\n").into_bytes(),
        format!("{text}\n2\n00:00:00,800 --> 00:00:00,900\nExtra\n").into_bytes(),
    ] {
        assert!(
            validate_worker_output(
                &output,
                &original,
                &timing_report(1, 0),
                &SyncConfig::default()
            )
            .is_err()
        );
    }
    assert!(
        validate_worker_output(
            text.as_bytes(),
            &original,
            &timing_report(2, 0),
            &SyncConfig::default()
        )
        .is_err()
    );
}

#[test]
fn refuses_config_and_delta_disagreements() {
    let text = "1\n00:00:00,100 --> 00:00:00,700\nAuthored\n";
    let original = SubtitleDocument::parse(text, None).unwrap();
    let config = SyncConfig::default();
    for field in [
        "global_offset_ms",
        "split_shift",
        "fps_ratio",
        "max_cue_delta_ms",
        "median_cue_delta_ms",
    ] {
        let mut report = timing_report(1, 0);
        match field {
            "global_offset_ms" => report.global_offset_ms = config.max_offset_ms + 1,
            "split_shift" => report.split_segments[0].shift_ms = config.max_split_offset_ms + 1,
            "fps_ratio" => report.fps_ratio = 2.0,
            "max_cue_delta_ms" => report.max_cue_delta_ms = 1,
            "median_cue_delta_ms" => report.median_cue_delta_ms = 1.0,
            _ => unreachable!(),
        }
        assert!(
            validate_worker_output(text.as_bytes(), &original, &report, &config).is_err(),
            "accepted inconsistent {field}"
        );
    }
    let mut report = timing_report(1, 0);
    report.fps_ratio = subsync_rs::align::FPS_RATIOS[1];
    let config = SyncConfig {
        fps_search: false,
        ..config
    };
    assert!(validate_worker_output(text.as_bytes(), &original, &report, &config).is_err());
}

#[test]
fn refuses_new_srt_overlap_even_when_delta_statistics_agree() {
    let text = "1\n00:00:00,100 --> 00:00:00,400\nOne\n\n2\n00:00:00,500 --> 00:00:00,800\nTwo\n";
    let original = SubtitleDocument::parse(text, None).unwrap();
    let output = text
        .replace("00,100", "00,200")
        .replace("00,400", "00,700")
        .replace("00,500", "00,600")
        .replace("00,800", "00,900");
    assert!(
        validate_worker_output(
            output.as_bytes(),
            &original,
            &timing_report(2, 100),
            &SyncConfig::default()
        )
        .is_err()
    );
}

#[test]
fn accepts_real_engine_fps_quantization_and_negative_clamping_reports() {
    use subsync_rs::vad::{TimeSpan, VadResult};

    let intervals = [
        (1520_u64, 2470_u64),
        (3110, 3890),
        (4990, 6370),
        (6780, 7390),
        (8020, 9150),
        (10590, 12420),
        (13160, 13840),
        (14760, 16040),
        (24530, 25640),
        (27200, 28080),
        (28880, 29590),
        (31230, 32980),
        (33740, 34650),
        (35980, 37080),
        (37680, 38270),
        (39300, 40680),
    ];
    let mut activity = vec![0_u8; 4500];
    for &(start, end) in &intervals {
        activity[start as usize / 10..end as usize / 10].fill(1);
    }
    let vad = VadResult {
        frame_ms: 10,
        duration_ms: 45000,
        probability: activity.iter().map(|&value| f32::from(value)).collect(),
        activity,
        segments: intervals
            .iter()
            .map(|&(start_ms, end_ms)| TimeSpan { start_ms, end_ms })
            .collect(),
    };
    for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
        for clamp in [false, true] {
            let time = |ms: u64| match format {
                SubtitleFormat::Srt => format!(
                    "00:{:02}:{:02},{:03}",
                    ms / 60000,
                    ms / 1000 % 60,
                    ms % 1000
                ),
                SubtitleFormat::Ass => format!(
                    "0:{:02}:{:02}.{:02}",
                    ms / 60000,
                    ms / 1000 % 60,
                    ms % 1000 / 10
                ),
            };
            let mut text = if format == SubtitleFormat::Ass {
                "[Events]\r\nFormat: End, Text, Start\r\n".to_owned()
            } else {
                String::new()
            };
            for (i, &(start, end)) in intervals.iter().enumerate() {
                let convert = |ms: u64| {
                    if clamp {
                        if i == 0 { ms - 1500 } else { ms + 2400 }
                    } else {
                        (ms as f64 * 25.0 / 23.976).round() as u64
                    }
                };
                let (start, end) = (time(convert(start)), time(convert(end)));
                if format == SubtitleFormat::Srt {
                    text.push_str(&format!(
                        "{}\r\n{start} --> {end}\r\nAuthored {i}\r\n\r\n",
                        i + 1
                    ));
                } else {
                    text.push_str(&format!("Dialogue: {end},Authored {i},{start}\r\n"));
                }
            }
            let original = SubtitleDocument::parse(&text, None).unwrap();
            let mut aligned = SubtitleDocument::parse(&text, None).unwrap();
            let config = SyncConfig {
                fps_search: !clamp,
                max_split_offset_ms: 0,
                snap_ms: 0,
                ..SyncConfig::default()
            };
            let report = subsync_rs::sync_subtitles_with_vad(&mut aligned, &vad, &config).unwrap();
            if clamp {
                assert!(report.global_offset_ms < 0, "{report:?}");
                assert_eq!(aligned.cues[0].start_ms, 0);
            } else {
                assert!(
                    (report.fps_ratio - 23.976 / 25.0).abs() < 1e-9,
                    "{report:?}"
                );
            }
            let bytes = aligned.serialize().unwrap();
            let (_, parsed) = parse_report(&serde_json::to_string(&report).unwrap()).unwrap();
            validate_worker_output(bytes.as_bytes(), &original, &parsed, &config).unwrap();
        }
    }
}

#[test]
fn malformed_worker_report_is_refused() {
    assert!(parse_report("{\"cues_total\": 2, \"split_segments\": []}").is_err());
    assert!(parse_report("logging\n{}").is_err());
}

#[test]
fn stages_both_files_before_declaring_a_worker_result() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let output = root.path().join("output.srt");
    let staged = stage_file(&output, |file| {
        file.write_all(b"owned bytes")?;
        Ok(())
    })?;
    fs::write(&output, b"independent winner")?;
    assert!(staged.persist_noclobber(&output).is_err());
    assert_eq!(fs::read(&output)?, b"independent winner");
    Ok(())
}
