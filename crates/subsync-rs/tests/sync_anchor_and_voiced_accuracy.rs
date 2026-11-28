use std::f64::consts::TAU;
use subsync_rs::align::validate_reported_timing;
use subsync_rs::vad::{TimeSpan, VadResult, extract_speech_activity};
use subsync_rs::{
    SubtitleDocument, SubtitleFormat, SyncConfig, SyncError, sync_subtitles_with_vad,
};

const RATE: usize = 16_000;
const CONFIRMED: &[(f64, f64)] = &[(400.0, 3500.0), (1200.0, 3500.0), (2400.0, 3500.0)];

fn build_vad(
    duration_ms: u64,
    spans: &[(u64, u64, f32)],
    voiced_spans: &[(u64, u64)],
) -> VadResult {
    let frames = (duration_ms / 10) as usize;
    let mut activity = vec![0_u8; frames];
    let mut probability = vec![0.0_f32; frames];
    let mut segments = Vec::new();
    for &(start_ms, end_ms, prob) in spans {
        let a = (start_ms / 10) as usize;
        let b = (end_ms / 10) as usize;
        activity[a..b].fill(1);
        probability[a..b].fill(prob);
        segments.push(TimeSpan { start_ms, end_ms });
    }
    let mut voiced_activity = vec![0_u8; frames];
    for &(start_ms, end_ms) in voiced_spans {
        let a = (start_ms / 10) as usize;
        let b = (end_ms / 10) as usize;
        voiced_activity[a..b].fill(1);
    }
    VadResult {
        frame_ms: 10,
        duration_ms,
        probability,
        activity,
        voiced_activity,
        raw_segments: segments.clone(),
        segments,
        onset_anchors_ms: Vec::new(),
    }
}

fn fixed_config() -> SyncConfig {
    SyncConfig {
        max_offset_ms: 0,
        fps_search: false,
        max_split_offset_ms: 1500,
        snap_ms: 450,
        lead_in_ms: 70,
        end_adjust_ms: 600,
        ..SyncConfig::default()
    }
}

#[test]
fn anchored_cue_does_not_jump_across_silence_to_louder_neighbor() {
    // Mirrors Birds of Prey #1072 / #632 / #757: cue sits squarely on its own speech onset
    // [2020..2760], followed by a louder unrelated burst [3100..4480].
    let vad = build_vad(
        7000,
        &[(2020, 2760, 0.50), (3100, 4480, 0.95)],
        &[(2020, 2760), (3100, 4480)],
    );
    let mut doc =
        SubtitleDocument::parse("1\n00:00:02,000 --> 00:00:03,300\nCome on.\n", None).unwrap();
    let original = doc.cues.clone();
    let config = fixed_config();
    let report = sync_subtitles_with_vad(&mut doc, &vad, &config).unwrap();
    assert!(
        doc.cues[0].start_ms.abs_diff(2000) <= 80,
        "anchored cue jumped to {:?}",
        doc.cues[0]
    );
    validate_reported_timing(&original, &doc.cues, SubtitleFormat::Srt, &report, &config).unwrap();
}

#[test]
fn continuous_loud_wall_does_not_slide_aligned_cue_backward() {
    // Mirrors Birds of Prey #800 / #1073 / #191: continuous speech/noise wall where an earlier
    // slice has higher raw probability than the cue's own window.
    let mut vad = build_vad(9000, &[(500, 7500, 0.45)], &[(500, 7500)]);
    vad.probability[100..300].fill(0.95);
    let mut doc = SubtitleDocument::parse(
        "1\n00:00:03,500 --> 00:00:05,800\nContinuous scene line.\n",
        None,
    )
    .unwrap();
    let original = doc.cues.clone();
    let config = SyncConfig {
        snap_ms: 0,
        end_adjust_ms: 0,
        ..fixed_config()
    };
    let report = sync_subtitles_with_vad(&mut doc, &vad, &config).unwrap();
    assert_eq!(doc.cues[0].start_ms, 3500);
    assert_eq!(doc.cues[0].end_ms, 5800);
    validate_reported_timing(&original, &doc.cues, SubtitleFormat::Srt, &report, &config).unwrap();
}

#[test]
fn backward_snap_refuses_pre_speech_unvoiced_hiss() {
    // Mirrors Birds of Prey #633 / #651 / #1087 / #1096: segment begins 140 ms early on unvoiced
    // hiss (voiced_activity = 0) while genuine voiced speech begins at the cue start (3000 ms).
    let vad = build_vad(7000, &[(2860, 4400, 0.70)], &[(3000, 4400)]);
    let mut doc =
        SubtitleDocument::parse("1\n00:00:03,000 --> 00:00:04,600\nSpeech start.\n", None).unwrap();
    let original = doc.cues.clone();
    let config = SyncConfig {
        max_split_offset_ms: 0,
        end_adjust_ms: 0,
        ..fixed_config()
    };
    let report = sync_subtitles_with_vad(&mut doc, &vad, &config).unwrap();
    assert_eq!(doc.cues[0].start_ms, 3000);
    validate_reported_timing(&original, &doc.cues, SubtitleFormat::Srt, &report, &config).unwrap();
}

#[test]
fn forward_snap_refuses_skipping_active_voiced_speech() {
    // Mirrors Birds of Prey #33: deep voiced speech runs at 2000..2350 ms before primary VAD
    // confirms at 2350 ms; snap_start must not push the cue 280 ms late past its voiced start.
    let vad = build_vad(7000, &[(2350, 3200, 0.85)], &[(2000, 3200)]);
    let mut doc =
        SubtitleDocument::parse("1\n00:00:02,000 --> 00:00:03,200\nA new me.\n", None).unwrap();
    let original = doc.cues.clone();
    let config = SyncConfig {
        max_split_offset_ms: 0,
        end_adjust_ms: 0,
        ..fixed_config()
    };
    let report = sync_subtitles_with_vad(&mut doc, &vad, &config).unwrap();
    assert_eq!(doc.cues[0].start_ms, 2000);
    validate_reported_timing(&original, &doc.cues, SubtitleFormat::Srt, &report, &config).unwrap();
}

#[test]
fn quiet_scene_sibilant_onset_backtracks_without_triggering_in_noise() {
    // Mirrors Birds of Prey #165 ("Soft, toasted bun..."): a crisp high-frequency sibilant
    // emerges from quiet silence directly into the confirmed vowel.
    let mut pcm = vec![0_i16; 7 * RATE];
    for (i, sample) in pcm.iter_mut().enumerate().take(2300 * 16).skip(2050 * 16) {
        // Deterministic high-ZCR sibilant (~5.2 kHz + 6.4 kHz, ZCR > 5000 Hz, ~-27 dB).
        let t = i as f64 / RATE as f64;
        *sample = (1400.0 * (TAU * 5200.0 * t).sin() + 1000.0 * (TAU * 6400.0 * t).sin()) as i16;
    }
    for (i, sample) in pcm.iter_mut().enumerate().take(3400 * 16).skip(2300 * 16) {
        let t = i as f64 / RATE as f64;
        *sample = CONFIRMED
            .iter()
            .map(|&(hz, gain)| gain * (TAU * hz * t).sin())
            .sum::<f64>() as i16;
    }
    let vad = extract_speech_activity(&pcm, RATE as u32, 10);
    assert_eq!(vad.segments.len(), 1);
    assert!(vad.segments[0].start_ms >= 2280);
    assert!(
        vad.onset_anchors_ms[0] <= 2070,
        "expected sibilant onset near 2050 ms, got {:?}",
        vad.onset_anchors_ms
    );
}

#[test]
fn voiced_tail_and_long_cue_reading_margin_prevent_premature_end_chop() {
    // Mirrors Birds of Prey #788 / #759 / #650 (quiet voiced tail continues after primary VAD)
    // and #171 (660 ms reading margin on a 3610 ms cue).
    let vad_voiced_tail = build_vad(7000, &[(1000, 1900, 0.85)], &[(1000, 2750)]);
    let mut doc1 =
        SubtitleDocument::parse("1\n00:00:01,000 --> 00:00:02,900\nQuiet tail.\n", None).unwrap();
    let config = SyncConfig {
        max_split_offset_ms: 0,
        snap_ms: 0,
        ..fixed_config()
    };
    sync_subtitles_with_vad(&mut doc1, &vad_voiced_tail, &config).unwrap();
    assert_eq!(doc1.cues[0].end_ms, 2900);

    let vad_long_reading = build_vad(7000, &[(1000, 3950, 0.85)], &[(1000, 3950)]);
    let mut doc2 =
        SubtitleDocument::parse("1\n00:00:01,000 --> 00:00:04,610\nLong line.\n", None).unwrap();
    sync_subtitles_with_vad(&mut doc2, &vad_long_reading, &config).unwrap();
    assert_eq!(doc2.cues[0].end_ms, 4610);
}

#[test]
fn malformed_voiced_activity_is_rejected() {
    let mut vad = build_vad(4000, &[(1000, 2200, 0.9)], &[(1000, 2200)]);
    vad.voiced_activity.push(0);
    let mut doc = SubtitleDocument::parse("1\n00:00:01,000 --> 00:00:02,200\nHi\n", None).unwrap();
    assert!(matches!(
        sync_subtitles_with_vad(&mut doc, &vad, &fixed_config()),
        Err(SyncError::Audio(_))
    ));
    vad.voiced_activity.pop();
    vad.voiced_activity[0] = 2;
    assert!(matches!(
        sync_subtitles_with_vad(&mut doc, &vad, &fixed_config()),
        Err(SyncError::Audio(_))
    ));
}
