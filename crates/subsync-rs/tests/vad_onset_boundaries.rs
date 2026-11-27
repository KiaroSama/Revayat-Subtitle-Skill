use std::f64::consts::TAU;
use subsync_rs::align::validate_reported_timing;
use subsync_rs::vad::{VadResult, extract_speech_activity};
use subsync_rs::{SubtitleDocument, SubtitleFormat, SyncConfig, sync_subtitles_with_vad};

const RATE: usize = 16_000;
const CONFIRMED: &[(f64, f64)] = &[(400.0, 3500.0), (1200.0, 3500.0), (2400.0, 3500.0)];
const QUIET: &[(f64, f64)] = &[
    (123.0, 450.0),
    (246.0, 360.0),
    (369.0, 150.0),
    (738.0, 135.0),
];
const LOUD: &[(f64, f64)] = &[
    (123.0, 1500.0),
    (246.0, 1200.0),
    (369.0, 500.0),
    (738.0, 450.0),
];

type ToneSpan<'a> = (usize, usize, &'a [(f64, f64)]);

fn detect(parts: &[ToneSpan<'_>], reset_phase: bool) -> VadResult {
    let mut pcm = vec![0; 8 * RATE];
    for &(start, end, tones) in parts {
        let first = start * RATE / 1000;
        for (i, sample) in pcm
            .iter_mut()
            .enumerate()
            .take(end * RATE / 1000)
            .skip(first)
        {
            let phase = if reset_phase { i - first } else { i };
            *sample = tones
                .iter()
                .map(|&(hz, gain)| gain * (TAU * hz * phase as f64 / RATE as f64).sin())
                .sum::<f64>() as i16;
        }
    }
    extract_speech_activity(&pcm, RATE as u32, 10)
}

fn assert_snapped_start(vad: &VadResult, start: u64, end: u64, expected: u64) {
    let config = SyncConfig {
        max_offset_ms: 0,
        max_split_offset_ms: 0,
        fps_search: false,
        end_adjust_ms: 0,
        ..SyncConfig::default()
    };
    for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
        let source = match format {
            SubtitleFormat::Srt => format!(
                "1\n00:00:{:02},{:03} --> 00:00:{:02},{:03}\nAuthored dialogue\n",
                start / 1000,
                start % 1000,
                end / 1000,
                end % 1000
            ),
            SubtitleFormat::Ass => format!(
                "[Events]\nFormat: Start, End, Text\nDialogue: 0:00:{:02}.{:02},0:00:{:02}.{:02},Authored dialogue\n",
                start / 1000,
                start % 1000 / 10,
                end / 1000,
                end % 1000 / 10
            ),
        };
        let mut doc = SubtitleDocument::parse(&source, None).unwrap();
        let original = doc.cues.clone();
        let report = sync_subtitles_with_vad(&mut doc, vad, &config).unwrap();
        assert_eq!(doc.cues[0].start_ms, expected, "{format:?}");
        assert_eq!(doc.cues[0].end_ms, end, "{format:?}");
        assert_eq!(report.global_offset_ms, 0);
        assert_eq!(report.fps_ratio, 1.0);
        assert!(report.split_segments.iter().all(|part| part.shift_ms == 0));
        assert_eq!(report.cues_snapped, 1);
        assert_eq!(report.cues_end_adjusted, 0);
        validate_reported_timing(&original, &doc.cues, format, &report, &config).unwrap();
    }
}

#[test]
fn stationary_quiet_bed_cannot_invent_an_attack_at_the_lookback_boundary() {
    for (bed_end, primary_start, primary_end, expected) in
        [(2300, 2300, 3400, 2240), (2400, 2500, 3600, 2440)]
    {
        let speech = (primary_start, primary_end, CONFIRMED);
        let control = detect(&[speech], false);
        let vad = detect(&[(500, bed_end, QUIET), speech], false);
        assert_eq!(vad.segments, control.segments);
        assert_eq!(vad.raw_segments, control.raw_segments);
        assert_eq!(vad.activity, control.activity);
        // The bed began at 500 ms; no acoustic attack occurs inside this lookback window.
        assert_eq!(vad.onset_anchors_ms, [vad.segments[0].start_ms]);
        assert_snapped_start(
            &vad,
            (primary_start - 100) as u64,
            primary_end as u64,
            expected,
        );
    }
}

#[test]
fn a_run_clipped_by_the_previous_segment_does_not_create_a_new_attack() {
    let vad = detect(
        &[
            (1000, 1900, CONFIRMED),
            (1900, 2300, QUIET),
            (2300, 3400, CONFIRMED),
        ],
        false,
    );
    assert_eq!(vad.segments.len(), 2);
    assert_eq!(vad.segments[0].end_ms, 2010);
    assert_eq!(vad.segments[1].start_ms, 2290);
    // Sustained audio across the previous segment's tail has no onset at 2010 ms.
    assert_eq!(vad.onset_anchors_ms[1], 2290);
    assert_snapped_start(&vad, 2200, 3400, 2240);
}

#[test]
fn an_attack_outside_the_lookback_is_not_retimestamped_at_its_boundary() {
    let vad = detect(&[(1890, 2300, QUIET), (2300, 3400, CONFIRMED)], false);
    assert_eq!(vad.segments.len(), 1);
    assert_eq!(vad.segments[0].start_ms, 2290);
    assert_eq!(vad.onset_anchors_ms, [2290]);
    assert_snapped_start(&vad, 2200, 3400, 2240);
}

#[test]
fn real_attacks_at_and_inside_the_lookback_boundary_remain_recoverable() {
    for (attack, end, primary, primary_end) in [
        (1910, 2300, 2300, 3400),
        (1920, 2300, 2300, 3400),
        (2000, 2150, 2300, 3400),
        (2110, 2400, 2500, 3600),
    ] {
        let vad = detect(
            &[(attack, end, QUIET), (primary, primary_end, CONFIRMED)],
            false,
        );
        assert_eq!(vad.segments.len(), 1);
        assert_eq!(vad.onset_anchors_ms, [attack as u64], "attack={attack}");
        assert_snapped_start(
            &vad,
            (primary - 100) as u64,
            primary_end as u64,
            (attack - 50) as u64,
        );
    }
}

#[test]
fn real_attacks_at_and_after_the_previous_activity_end_are_preserved() {
    for attack in [2010, 2020] {
        // Start each waveform at zero phase to avoid a separate discontinuity impulse.
        let vad = detect(
            &[
                (1000, 1900, CONFIRMED),
                (attack, 2300, QUIET),
                (2300, 3400, CONFIRMED),
            ],
            true,
        );
        assert_eq!(vad.segments.len(), 2);
        assert_eq!(vad.segments[0].end_ms, 2010);
        assert_eq!(vad.onset_anchors_ms[1], attack as u64);
        assert_snapped_start(&vad, 2200, 3400, (attack - 50) as u64);
    }
}

#[test]
fn loud_stationary_bed_keeps_the_confirmed_onset() {
    let vad = detect(&[(500, 2300, LOUD), (2300, 3400, CONFIRMED)], false);
    assert_eq!(vad.segments.len(), 1);
    assert_eq!(vad.onset_anchors_ms, [vad.segments[0].start_ms]);
    assert_snapped_start(&vad, 2200, 3400, 2250);
}
