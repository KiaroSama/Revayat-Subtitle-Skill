use subsync_rs::align::validate_reported_timing;
use subsync_rs::vad::{TimeSpan, VadResult};
use subsync_rs::{
    SplitSegment, SubtitleCue, SubtitleDocument, SubtitleFormat, SyncConfig, SyncReport,
    sync_subtitles_with_vad,
};

fn fixed_timing(lead_in_ms: u64) -> SyncConfig {
    SyncConfig {
        max_offset_ms: 0,
        max_split_offset_ms: 0,
        fps_search: false,
        lead_in_ms,
        end_adjust_ms: 0,
        ..SyncConfig::default()
    }
}

#[test]
fn snap_skips_a_nearer_short_burst_that_would_reduce_speech_overlap() {
    let segments = vec![
        TimeSpan {
            start_ms: 1150,
            end_ms: 1210,
        },
        TimeSpan {
            start_ms: 1800,
            end_ms: 2600,
        },
    ];
    let mut activity = vec![0_u8; 300];
    for segment in &segments {
        activity[segment.start_ms as usize / 10..segment.end_ms as usize / 10].fill(1);
    }
    let vad = VadResult {
        raw_segments: Vec::new(),
        onset_anchors_ms: Vec::new(),
        frame_ms: 10,
        duration_ms: 3000,
        probability: activity.iter().map(|&value| f32::from(value)).collect(),
        activity,
        segments,
    };
    for (lead, expected_start, expected_snaps) in [(0, 1400, 0), (50, 1750, 1)] {
        let config = fixed_timing(lead);
        let mut doc =
            SubtitleDocument::parse("1\n00:00:01,400 --> 00:00:02,600\nAuthored cue\n", None)
                .unwrap();
        let original = doc.cues.clone();
        let report = sync_subtitles_with_vad(&mut doc, &vad, &config).unwrap();
        assert_eq!(doc.cues[0].start_ms, expected_start, "lead={lead}");
        assert_eq!(doc.cues[0].end_ms, 2600);
        assert_eq!(report.cues_snapped, expected_snaps);
        assert!(report.overlap_score_after >= report.overlap_score_before);
        validate_reported_timing(&original, &doc.cues, doc.format, &report, &config).unwrap();
    }
}

#[test]
fn replay_requires_the_implied_onset_to_be_inside_the_audio() {
    let input = [SubtitleCue {
        start_ms: 3900,
        end_ms: 4900,
    }];
    let output = [SubtitleCue {
        start_ms: 3950,
        end_ms: 4900,
    }];
    let config = fixed_timing(50);
    let mut report = SyncReport {
        audio_duration_ms: 4000,
        speech_segments_count: 1,
        cues_total: 1,
        fps_ratio: 1.0,
        global_offset_ms: 0,
        split_segments: vec![SplitSegment {
            start_cue: 1,
            end_cue: 1,
            shift_ms: 0,
        }],
        cues_snapped: 1,
        cues_end_adjusted: 0,
        median_cue_delta_ms: 50.0,
        max_cue_delta_ms: 50,
        overlap_score_before: 0.0,
        overlap_score_after: 0.0,
        video_path: None,
        wav_path: None,
        subtitle_in: None,
        subtitle_out: None,
    };
    for format in [SubtitleFormat::Ass, SubtitleFormat::Srt] {
        for (duration, expected) in [(4000, false), (3999, false), (4001, true)] {
            report.audio_duration_ms = duration;
            assert_eq!(
                validate_reported_timing(&input, &output, format, &report, &config).is_ok(),
                expected,
                "format={format:?}, duration={duration}"
            );
        }
    }
}

#[test]
fn endpoints_extend_trim_and_replay() {
    let segments = [(1000, 1800), (3000, 3500), (5000, 6000)]
        .map(|(start_ms, end_ms)| TimeSpan { start_ms, end_ms })
        .to_vec();
    let mut activity = vec![0; 800];
    for span in &segments {
        activity[span.start_ms as usize / 10..span.end_ms as usize / 10].fill(1);
    }
    let vad = VadResult {
        raw_segments: Vec::new(),
        onset_anchors_ms: Vec::new(),
        frame_ms: 10,
        duration_ms: 8000,
        probability: activity.iter().map(|&a| f32::from(a)).collect(),
        activity,
        segments,
    };
    let mut config = fixed_timing(50);
    config.snap_ms = 0;
    config.end_adjust_ms = 1000;
    let mut doc = SubtitleDocument::parse(
        "1
00:00:00,950 --> 00:00:01,650
One

2
00:00:02,950 --> 00:00:03,550
Two

3
00:00:04,950 --> 00:00:06,800
Three
",
        None,
    )
    .unwrap();
    let original = doc.cues.clone();
    let report = sync_subtitles_with_vad(&mut doc, &vad, &config).unwrap();
    assert_eq!(
        doc.cues.iter().map(|cue| cue.end_ms).collect::<Vec<_>>(),
        [2050, 3850, 6250]
    );
    assert_eq!(report.cues_end_adjusted, 3);
    let replay = |cues: &[SubtitleCue], report: &SyncReport| {
        validate_reported_timing(&original, cues, SubtitleFormat::Srt, report, &config)
    };
    replay(&doc.cues, &report).unwrap();
    let mut forged = doc.cues.clone();
    forged[0].end_ms = 3000;
    assert!(replay(&forged, &report).is_err());
    let mut forged_report = report;
    forged_report.cues_end_adjusted = 2;
    assert!(replay(&doc.cues, &forged_report).is_err());
}

#[test]
fn recovers_large_standalone_drift_without_tearing_joined_cues() {
    let segments = [(1000, 2100), (2650, 2800), (5000, 6100), (6130, 7230)]
        .map(|(start_ms, end_ms)| TimeSpan { start_ms, end_ms })
        .to_vec();
    let mut activity = vec![0; 900];
    for span in &segments {
        activity[span.start_ms as usize / 10..span.end_ms as usize / 10].fill(1);
    }
    let vad = VadResult {
        raw_segments: Vec::new(),
        onset_anchors_ms: Vec::new(),
        frame_ms: 10,
        duration_ms: 9000,
        probability: activity.iter().map(|&a| f32::from(a)).collect(),
        activity,
        segments,
    };
    let mut config = fixed_timing(50);
    config.max_split_offset_ms = 1800;
    config.end_adjust_ms = 600;
    let mut doc = SubtitleDocument::parse(
        "1
00:00:02,150 --> 00:00:03,250
One

2
00:00:05,350 --> 00:00:06,450
Two

3
00:00:06,480 --> 00:00:07,580
Three
",
        None,
    )
    .unwrap();
    let original = doc.cues.clone();
    let report = sync_subtitles_with_vad(&mut doc, &vad, &config).unwrap();
    assert_eq!(doc.cues.len(), 3);
    assert!(doc.cues[0].start_ms.abs_diff(950) <= 60, "{:?}", doc.cues);
    assert!((0..=110).contains(&(doc.cues[2].start_ms as i64 - doc.cues[1].end_ms as i64)));
    validate_reported_timing(&original, &doc.cues, SubtitleFormat::Srt, &report, &config).unwrap();
}
