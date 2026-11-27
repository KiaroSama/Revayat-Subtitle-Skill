use subsync_rs::align::validate_reported_timing;
use subsync_rs::vad::{TimeSpan, VadResult};
use subsync_rs::{
    SplitSegment, SubtitleCue, SubtitleDocument, SubtitleFormat, SyncConfig, SyncReport,
    sync_subtitles_with_vad,
};

fn report(fps_ratio: f64, shift_ms: i64, snapped: usize, adjusted: usize) -> SyncReport {
    SyncReport {
        audio_duration_ms: 4000,
        speech_segments_count: 1,
        cues_total: 1,
        fps_ratio,
        global_offset_ms: 0,
        split_segments: vec![SplitSegment {
            start_cue: 1,
            end_cue: 1,
            shift_ms,
        }],
        cues_snapped: snapped,
        cues_end_adjusted: adjusted,
        median_cue_delta_ms: 0.0,
        max_cue_delta_ms: 0,
        overlap_score_before: 0.0,
        overlap_score_after: 0.0,
        video_path: None,
        wav_path: None,
        subtitle_in: None,
        subtitle_out: None,
    }
}

#[test]
fn an_end_extension_cannot_enable_a_start_snap_that_preceded_it() {
    let input = [SubtitleCue {
        start_ms: 1000,
        end_ms: 2000,
    }];
    let config = SyncConfig {
        max_offset_ms: 0,
        max_split_offset_ms: 100,
        snap_ms: 1500,
        end_adjust_ms: 600,
        ..SyncConfig::default()
    };
    for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
        for (ratio, shift, expected_end) in [(1.0, 0, 2000_u64), (24.0 / 25.0, 100, 2020)] {
            let unit = if format == SubtitleFormat::Srt { 1 } else { 10 };
            for (start, accepted) in [
                (expected_end - 500, true),
                (expected_end - 500 + unit, false),
                (expected_end, false),
                (expected_end + 100, false),
            ] {
                let output = [SubtitleCue {
                    start_ms: start,
                    end_ms: expected_end + 600,
                }];
                assert_eq!(
                    validate_reported_timing(
                        &input,
                        &output,
                        format,
                        &report(ratio, shift, 1, 1),
                        &config,
                    )
                    .is_ok(),
                    accepted,
                    "format={format:?}, ratio={ratio}, shift={shift}, start={start}"
                );
            }
        }
    }
}

#[test]
fn a_valid_end_adjustment_still_requires_readable_final_duration() {
    let input = [SubtitleCue {
        start_ms: 1000,
        end_ms: 2000,
    }];
    let config = SyncConfig {
        max_offset_ms: 0,
        max_split_offset_ms: 0,
        fps_search: false,
        snap_ms: 500,
        end_adjust_ms: 600,
        ..SyncConfig::default()
    };
    for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
        for (end, accepted) in [(1950, false), (2050, true)] {
            let output = [SubtitleCue {
                start_ms: 1500,
                end_ms: end,
            }];
            assert_eq!(
                validate_reported_timing(&input, &output, format, &report(1.0, 0, 1, 1), &config,)
                    .is_ok(),
                accepted,
                "format={format:?}, end={end}"
            );
        }
    }
}

#[test]
fn real_snap_then_end_adjustment_replays_at_the_presnap_duration_boundary() {
    let config = SyncConfig {
        max_offset_ms: 0,
        max_split_offset_ms: 0,
        fps_search: false,
        snap_ms: 700,
        end_adjust_ms: 600,
        lead_in_ms: 50,
        ..SyncConfig::default()
    };
    for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
        let source = if format == SubtitleFormat::Srt {
            "1\n00:00:01,000 --> 00:00:02,000\nAuthored\n"
        } else {
            "[Events]\nFormat: Start, End, Text\nDialogue: 0:00:01.00,0:00:02.00,Authored\n"
        };
        for (onset, expected_start, snapped) in [(1550, 1500, 1), (1560, 1000, 0)] {
            let mut activity = vec![0_u8; 400];
            activity[onset / 10..220].fill(1);
            let vad = VadResult {
                frame_ms: 10,
                duration_ms: 4000,
                probability: activity.iter().map(|&value| f32::from(value)).collect(),
                activity,
                segments: vec![TimeSpan {
                    start_ms: onset as u64,
                    end_ms: 2200,
                }],
            };
            let mut doc = SubtitleDocument::parse(source, None).unwrap();
            let original = doc.cues.clone();
            let report = sync_subtitles_with_vad(&mut doc, &vad, &config).unwrap();
            assert_eq!(doc.cues[0].start_ms, expected_start);
            assert_eq!(doc.cues[0].end_ms, 2450);
            assert_eq!(report.cues_snapped, snapped);
            assert_eq!(report.cues_end_adjusted, 1);
            validate_reported_timing(&original, &doc.cues, format, &report, &config).unwrap();
        }
    }
}
