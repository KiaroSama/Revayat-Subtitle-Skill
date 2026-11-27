use super::*;
use subsync_rs::{SplitSegment, SubtitleCue};

fn endpoints(
    format: SubtitleFormat,
    input: &[(u64, u64)],
    output: &[(u64, u64)],
    config: &SyncConfig,
    mut report: SyncReport,
) -> anyhow::Result<()> {
    report.audio_duration_ms = 10000;
    let time = |ms: u64| match format {
        SubtitleFormat::Srt => format!(
            "{:02}:{:02}:{:02},{:03}",
            ms / 3600000,
            ms / 60000 % 60,
            ms / 1000 % 60,
            ms % 1000
        ),
        SubtitleFormat::Ass => format!(
            "{}:{:02}:{:02}.{:02}",
            ms / 3600000,
            ms / 60000 % 60,
            ms / 1000 % 60,
            ms % 1000 / 10
        ),
    };
    let mut source = if format == SubtitleFormat::Ass {
        "[Events]\nFormat: End, Text, Start\n".to_owned()
    } else {
        String::new()
    };
    for (i, &(start, end)) in input.iter().enumerate() {
        let (start, end) = (time(start), time(end));
        if format == SubtitleFormat::Ass {
            source.push_str(&format!("Dialogue: {end},Authored {i},{start}\n"));
        } else {
            source.push_str(&format!("{}\n{start} --> {end}\nAuthored {i}\n\n", i + 1));
        }
    }
    let original = SubtitleDocument::parse(&source, None)?;
    let mut document = SubtitleDocument::parse(&source, None)?;
    document.cues = output
        .iter()
        .map(|&(start_ms, end_ms)| SubtitleCue { start_ms, end_ms })
        .collect();
    let bytes = document.serialize()?;
    let emitted = SubtitleDocument::parse(&bytes, None)?;
    let mut deltas: Vec<_> = original
        .cues
        .iter()
        .zip(&emitted.cues)
        .map(|(a, b)| a.start_ms.abs_diff(b.start_ms))
        .collect();
    deltas.sort_unstable();
    let mid = deltas.len() / 2;
    report.max_cue_delta_ms = *deltas.last().unwrap();
    report.median_cue_delta_ms = if deltas.len().is_multiple_of(2) {
        (deltas[mid - 1] as f64 + deltas[mid] as f64) / 2.0
    } else {
        deltas[mid] as f64
    };
    validate_worker_output(bytes.as_bytes(), &original, &report, config)
}

fn split(start_cue: usize, end_cue: usize, shift_ms: i64) -> SplitSegment {
    SplitSegment {
        start_cue,
        end_cue,
        shift_ms,
    }
}

#[test]
fn zero_limits_accept_unchanged_timing_and_exact_cumulative_srt_repairs() {
    for (input, output) in [
        (vec![(1000, 2000)], vec![(1000, 2000)]),
        (vec![(0, 10000), (100, 200)], vec![(0, 100), (100, 200)]),
        (vec![(0, 1); 4], vec![(0, 1), (1, 2), (2, 3), (3, 4)]),
    ] {
        endpoints(
            SubtitleFormat::Srt,
            &input,
            &output,
            &zero_limits(),
            timing_report(input.len(), 0),
        )
        .unwrap();
    }
    assert!(
        endpoints(
            SubtitleFormat::Srt,
            &[(0, 10000), (100, 200)],
            &[(0, 99), (100, 200)],
            &zero_limits(),
            timing_report(2, 0)
        )
        .is_err()
    );
}

#[test]
fn exact_positive_and_negative_shift_boundaries_apply_to_both_endpoints() {
    let config = SyncConfig {
        end_adjust_ms: 0,
        max_offset_ms: 1000,
        max_split_offset_ms: 200,
        ..zero_limits()
    };
    for sign in [1, -1] {
        let mut report = timing_report(1, 0);
        report.global_offset_ms = sign * 1000;
        report.split_segments = vec![split(1, 1, sign * 1200)];
        let output = if sign > 0 { (3200, 4200) } else { (800, 1800) };
        endpoints(
            SubtitleFormat::Srt,
            &[(2000, 3000)],
            &[output],
            &config,
            report,
        )
        .unwrap();
        for bad in [(output.0 + 1, output.1), (output.0, output.1 + 1)] {
            let mut report = timing_report(1, 0);
            report.global_offset_ms = sign * 1000;
            report.split_segments = vec![split(1, 1, sign * 1200)];
            assert!(
                endpoints(
                    SubtitleFormat::Srt,
                    &[(2000, 3000)],
                    &[bad],
                    &config,
                    report
                )
                .is_err()
            );
        }
    }
}

#[test]
fn fps_then_total_split_shifts_and_format_rounding_are_replayed_exactly() {
    let config = SyncConfig {
        end_adjust_ms: 0,
        fps_search: true,
        max_split_offset_ms: 200,
        ..zero_limits()
    };
    let mut report = timing_report(2, 0);
    report.fps_ratio = 24.0 / 25.0;
    report.split_segments = vec![split(1, 1, 100), split(2, 2, -200)];
    endpoints(
        SubtitleFormat::Srt,
        &[(1000, 2000), (4000, 5000)],
        &[(1060, 2020), (3640, 4600)],
        &config,
        report,
    )
    .unwrap();
    let mut report = timing_report(2, 0);
    report.fps_ratio = 24.0 / 25.0;
    endpoints(
        SubtitleFormat::Ass,
        &[(260, 270), (270, 280)],
        &[(250, 260), (250, 260)],
        &config,
        report,
    )
    .unwrap();
    let mut report = timing_report(2, 0);
    report.fps_ratio = 24.0 / 25.0;
    report.split_segments = vec![split(1, 1, 0), split(2, 2, 10)];
    assert!(
        endpoints(
            SubtitleFormat::Ass,
            &[(260, 270), (270, 280)],
            &[(250, 260), (260, 270)],
            &config,
            report
        )
        .is_err()
    );
}

#[test]
fn ass_group_clamping_uses_time_order_and_preserves_internal_offsets() {
    let config = SyncConfig {
        end_adjust_ms: 0,
        max_offset_ms: 100,
        ..zero_limits()
    };
    for redundant in [false, true] {
        let mut report = timing_report(2, 0);
        report.global_offset_ms = -100;
        report.split_segments = if redundant {
            vec![split(1, 1, -100), split(2, 2, -100)]
        } else {
            vec![split(1, 2, -100)]
        };
        endpoints(
            SubtitleFormat::Ass,
            &[(50, 100), (30, 1030)],
            &[(20, 70), (0, 1000)],
            &config,
            report,
        )
        .unwrap();
    }
    let config = SyncConfig {
        end_adjust_ms: 0,
        max_split_offset_ms: 500,
        ..zero_limits()
    };
    let mut report = timing_report(2, 0);
    report.split_segments = vec![split(1, 1, 500), split(2, 2, -500)];
    endpoints(
        SubtitleFormat::Ass,
        &[(1200, 2200), (0, 1000)],
        &[(1500, 2500), (500, 1500)],
        &config,
        report,
    )
    .unwrap();
}

#[test]
fn non_centisecond_ass_report_shift_cannot_expand_the_requested_bound() {
    let config = SyncConfig {
        end_adjust_ms: 0,
        max_split_offset_ms: 31,
        ..zero_limits()
    };
    let mut report = timing_report(1, 0);
    report.split_segments = vec![split(1, 1, -31)];
    assert!(
        endpoints(
            SubtitleFormat::Ass,
            &[(1000, 2000)],
            &[(960, 1960)],
            &config,
            report
        )
        .is_err()
    );
}

#[test]
fn eligible_snap_boundaries_and_exact_remaining_duration_are_accepted() {
    for (start, distance) in [(900, 100), (1100, 100), (1500, 500)] {
        for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
            let config = SyncConfig {
                end_adjust_ms: 0,
                snap_ms: distance,
                ..zero_limits()
            };
            let mut report = timing_report(1, 0);
            report.cues_snapped = 1;
            endpoints(format, &[(1000, 2000)], &[(start, 2000)], &config, report).unwrap();
        }
    }
    let config = SyncConfig {
        end_adjust_ms: 0,
        snap_ms: 50,
        ..zero_limits()
    };
    let mut report = timing_report(2, 0);
    report.cues_snapped = 1;
    endpoints(
        SubtitleFormat::Srt,
        &[(0, 950), (1000, 2000)],
        &[(0, 950), (950, 2000)],
        &config,
        report,
    )
    .unwrap();
}

#[test]
fn real_engine_nonzero_snaps_obey_duration_without_a_new_onset_grid_restriction() {
    use subsync_rs::vad::{TimeSpan, VadResult};

    let source = "1\n00:00:01,000 --> 00:00:02,000\nAuthored\n";
    let original = SubtitleDocument::parse(source, None).unwrap();
    let config = SyncConfig {
        end_adjust_ms: 0,
        snap_ms: 100,
        lead_in_ms: 0,
        ..zero_limits()
    };
    // The detector emits 10 ms onsets, but the public VAD contract also accepts
    // subframe segment boundaries. Both remain valid for compatible helpers.
    for onset in [1050, 1055] {
        let mut activity = vec![0_u8; 300];
        activity[onset as usize / 10..200].fill(1);
        let vad = VadResult {
            frame_ms: 10,
            duration_ms: 3000,
            probability: activity.iter().map(|&value| f32::from(value)).collect(),
            activity,
            segments: vec![TimeSpan {
                start_ms: onset,
                end_ms: 2000,
            }],
        };
        let mut aligned = SubtitleDocument::parse(source, None).unwrap();
        let mut report = subsync_rs::sync_subtitles_with_vad(&mut aligned, &vad, &config).unwrap();
        assert_eq!(report.cues_snapped, 1);
        assert_eq!(aligned.cues[0].start_ms, onset);
        let output = aligned.serialize().unwrap();
        validate_worker_output(output.as_bytes(), &original, &report, &config).unwrap();
        report.audio_duration_ms = onset;
        assert!(validate_worker_output(output.as_bytes(), &original, &report, &config).is_err());
        report.audio_duration_ms = onset + 1;
        validate_worker_output(output.as_bytes(), &original, &report, &config).unwrap();
    }
}

#[test]
fn snapping_cannot_change_an_end_or_escape_distance_duration_and_count_limits() {
    for (start, end, distance, count) in [
        (899, 2000, 100, 1),
        (1101, 2000, 100, 1),
        (1501, 2000, 501, 1),
        (900, 2000, 100, 0),
        (1000, 2000, 100, 1),
        (1000, 2100, 100, 0),
    ] {
        let config = SyncConfig {
            end_adjust_ms: 0,
            snap_ms: distance,
            ..zero_limits()
        };
        let mut report = timing_report(1, 0);
        report.cues_snapped = count;
        assert!(
            endpoints(
                SubtitleFormat::Srt,
                &[(1000, 2000)],
                &[(start, end)],
                &config,
                report
            )
            .is_err()
        );
    }
    let config = SyncConfig {
        end_adjust_ms: 0,
        snap_ms: 100,
        ..zero_limits()
    };
    let mut report = timing_report(1, 0);
    report.cues_snapped = 1;
    assert!(
        endpoints(
            SubtitleFormat::Srt,
            &[(1000, 1450)],
            &[(900, 1450)],
            &config,
            report
        )
        .is_err()
    );
    let mut report = timing_report(2, 0);
    report.cues_snapped = 1;
    assert!(
        endpoints(
            SubtitleFormat::Srt,
            &[(0, 950), (1000, 2000)],
            &[(0, 950), (900, 2000)],
            &config,
            report
        )
        .is_err()
    );
    let mut report = timing_report(2, 0);
    report.cues_snapped = 1;
    assert!(
        endpoints(
            SubtitleFormat::Ass,
            &[(1000, 2000), (1100, 2100)],
            &[(900, 2000), (1100, 2100)],
            &config,
            report
        )
        .is_err()
    );
}

#[test]
fn public_replay_rejects_unsafe_inputs_before_arithmetic_or_indexing() {
    let valid = [SubtitleCue {
        start_ms: 1000,
        end_ms: 2000,
    }];
    for cues in [
        vec![],
        vec![SubtitleCue {
            start_ms: u64::MAX - 1,
            end_ms: u64::MAX,
        }],
        vec![SubtitleCue {
            start_ms: 2,
            end_ms: 1,
        }],
    ] {
        assert!(
            subsync_rs::align::validate_reported_timing(
                &cues,
                &valid,
                SubtitleFormat::Ass,
                &timing_report(1, 0),
                &zero_limits()
            )
            .is_err()
        );
    }
    let mut report = timing_report(1, 0);
    report.split_segments = vec![split(1, usize::MAX, 0)];
    assert!(
        subsync_rs::align::validate_reported_timing(
            &valid,
            &valid,
            SubtitleFormat::Srt,
            &report,
            &zero_limits()
        )
        .is_err()
    );
    let mut report = timing_report(1, 0);
    report.fps_ratio = f64::INFINITY;
    assert!(
        subsync_rs::align::validate_reported_timing(
            &valid,
            &valid,
            SubtitleFormat::Srt,
            &report,
            &zero_limits()
        )
        .is_err()
    );
}
