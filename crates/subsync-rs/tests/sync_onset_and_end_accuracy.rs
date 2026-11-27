use std::f64::consts::TAU;
use subsync_rs::vad::{VadResult, extract_speech_activity};
use subsync_rs::{SubtitleDocument, SyncConfig, sync_subtitles_with_vad};

const RATE: usize = 16_000;
const CONFIRMED: &[(f64, f64)] = &[(400.0, 3500.0), (1200.0, 3500.0), (2400.0, 3500.0)];
const HARMONICS: &[(f64, f64)] = &[
    (123.0, 1500.0),
    (246.0, 1200.0),
    (369.0, 500.0),
    (738.0, 450.0),
];

type ToneSpan<'a> = (usize, usize, &'a [(f64, f64)]);

fn detect(parts: &[ToneSpan<'_>]) -> VadResult {
    let mut pcm = vec![0; 8 * RATE];
    for &(start, end, tones) in parts {
        for (i, sample) in pcm.iter_mut().enumerate().take(end * 16).skip(start * 16) {
            *sample = tones
                .iter()
                .map(|&(hz, gain)| gain * (TAU * hz * i as f64 / RATE as f64).sin())
                .sum::<f64>() as i16;
        }
    }
    extract_speech_activity(&pcm, RATE as u32, 10)
}

fn sync(vad: &VadResult, start: &str, end: &str, snap_ms: u64, limit: u64) -> (u64, u64) {
    let config = SyncConfig {
        max_offset_ms: 0,
        max_split_offset_ms: 0,
        fps_search: false,
        snap_ms,
        end_adjust_ms: limit,
        ..SyncConfig::default()
    };
    let mut doc = SubtitleDocument::parse(
        &format!("1\n00:00:{start} --> 00:00:{end}\nDialogue\n"),
        None,
    )
    .unwrap();
    sync_subtitles_with_vad(&mut doc, vad, &config).unwrap();
    (doc.cues[0].start_ms, doc.cues[0].end_ms)
}

#[test]
fn recovers_voiced_attack_without_changing_the_primary_signal() {
    let vad = detect(&[(2000, 2150, HARMONICS), (2300, 3400, CONFIRMED)]);
    assert_eq!(vad.segments.len(), 1);
    assert!(vad.segments[0].start_ms >= 2280);
    assert_eq!(sync(&vad, "02,230", "03,400", 350, 0).0, 1950);
}

#[test]
fn bass_and_quiet_drone_cannot_create_or_backtrack_a_primary_onset() {
    for &(frequency, amplitude) in &[(125.0, 1800.0), (124.0, 250.0)] {
        let tone = [(frequency, amplitude)];
        assert!(detect(&[(2000, 2200, &tone)]).segments.is_empty());
        let vad = detect(&[(2000, 2200, &tone), (2300, 3400, CONFIRMED)]);
        assert_eq!(vad.segments.len(), 1);
        assert_eq!(sync(&vad, "02,200", "03,400", 350, 0).0, 2240);
    }
}

#[test]
fn a_second_run_crossing_the_confirmed_onset_vetoes_backtracking() {
    let vad = detect(&[
        (2000, 2150, HARMONICS),
        (2250, 2300, HARMONICS),
        (2300, 3400, CONFIRMED),
    ]);
    assert_eq!(vad.segments.len(), 1);
    assert_eq!(vad.onset_anchors_ms, [vad.segments[0].start_ms]);
}

#[test]
fn a_loud_flat_background_cannot_start_the_backtrack() {
    let attack = [
        (123.0, 2500.0),
        (246.0, 2000.0),
        (369.0, 833.0),
        (738.0, 750.0),
    ];
    let quiet = detect(&[(2000, 2200, &attack), (2300, 3400, CONFIRMED)]);
    let loud = detect(&[
        (1800, 2000, &[(70.0, 3300.0)]),
        (2000, 2200, &attack),
        (2300, 3400, CONFIRMED),
    ]);
    assert_eq!(quiet.segments.len(), 1);
    assert_eq!(loud.segments, quiet.segments);
    assert_eq!(quiet.onset_anchors_ms, [2000]);
    assert_eq!(loud.onset_anchors_ms, [loud.segments[0].start_ms]);
}

#[test]
fn merged_gap_does_not_extend_a_cue_into_a_second_speech_island() {
    let vad = detect(&[(1000, 2000, CONFIRMED), (2120, 3100, CONFIRMED)]);
    assert_eq!(vad.segments.len(), 1);
    assert_eq!(sync(&vad, "01,000", "02,050", 0, 600).1, 2050);
}

#[test]
fn one_raw_island_can_extend_a_cue_but_not_into_the_next_island() {
    let vad = detect(&[(1000, 2200, CONFIRMED), (2320, 3100, CONFIRMED)]);
    assert_eq!(vad.segments.len(), 1);
    assert_eq!(sync(&vad, "01,000", "02,000", 0, 600).1, 2290);
}

#[test]
fn minimum_dwell_cannot_push_an_extension_past_the_next_speech_island() {
    let vad = detect(&[(1000, 1300, CONFIRMED), (1420, 2300, CONFIRMED)]);
    assert_eq!(vad.segments.len(), 1);
    assert_eq!(sync(&vad, "01,000", "01,200", 0, 600).1, 1200);
}

#[test]
fn rejects_malformed_endpoint_evidence_without_retiming() {
    let source = "1\n00:00:01,050 --> 00:00:02,000\nDialogue\n";
    let check = |vad: &VadResult| {
        let mut doc = SubtitleDocument::parse(source, None).unwrap();
        assert!(matches!(
            sync_subtitles_with_vad(&mut doc, vad, &SyncConfig::default()),
            Err(subsync_rs::SyncError::Audio(_))
        ));
        assert_eq!(doc.serialize().unwrap(), source);
    };
    let mut vad = detect(&[(1000, 2200, CONFIRMED)]);
    vad.onset_anchors_ms.push(0);
    check(&vad);
    vad.onset_anchors_ms.clear();
    vad.raw_segments[0].end_ms = 0;
    check(&vad);
}

#[test]
fn retains_the_existing_silence_trim_budget() {
    let vad = detect(&[(1000, 2000, CONFIRMED)]);
    assert_eq!(sync(&vad, "01,000", "02,900", 0, 1000).1, 2360);
}
