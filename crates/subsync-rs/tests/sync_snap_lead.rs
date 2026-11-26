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
