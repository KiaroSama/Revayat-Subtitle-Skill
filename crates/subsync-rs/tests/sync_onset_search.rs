use std::f64::consts::TAU;
use subsync_rs::{
    SubtitleCue, SubtitleDocument, SubtitleFormat, SyncConfig,
    align::validate_reported_timing,
    sync_subtitles_with_vad,
    vad::{TimeSpan, VadResult, extract_speech_activity},
};

fn activity(spans: &[(u64, u64)], anchors: &[u64]) -> VadResult {
    let mut activity = vec![0_u8; 600];
    for &(start, end) in spans {
        activity[start as usize / 10..end.div_ceil(10) as usize].fill(1);
    }
    VadResult {
        frame_ms: 10,
        duration_ms: 6000,
        probability: activity.iter().map(|&value| f32::from(value)).collect(),
        activity,
        segments: spans
            .iter()
            .map(|&(start_ms, end_ms)| TimeSpan { start_ms, end_ms })
            .collect(),
        raw_segments: Vec::new(),
        onset_anchors_ms: anchors.to_vec(),
    }
}

fn cue_list(cues: &[(u64, u64)]) -> Vec<SubtitleCue> {
    cues.iter()
        .map(|&(start_ms, end_ms)| SubtitleCue { start_ms, end_ms })
        .collect()
}

fn sync(
    format: SubtitleFormat,
    cues: &[(u64, u64)],
    vad: &VadResult,
    snap_ms: u64,
    lead_in_ms: u64,
) -> Vec<SubtitleCue> {
    let time = |ms: u64| match format {
        SubtitleFormat::Srt => format!("00:00:{:02},{:03}", ms / 1000, ms % 1000),
        SubtitleFormat::Ass => format!("0:00:{:02}.{:02}", ms / 1000, ms % 1000 / 10),
    };
    let mut source = match format {
        SubtitleFormat::Srt => String::new(),
        SubtitleFormat::Ass => "[Events]\nFormat: Start, End, Text\n".to_owned(),
    };
    for (i, &(start, end)) in cues.iter().enumerate() {
        let (start, end) = (time(start), time(end));
        match format {
            SubtitleFormat::Srt => {
                source.push_str(&format!("{}\n{start} --> {end}\nAuthored {i}\n\n", i + 1));
            }
            SubtitleFormat::Ass => {
                source.push_str(&format!("Dialogue: {start},{end},Authored {i}\n"));
            }
        }
    }
    let config = SyncConfig {
        max_offset_ms: 0,
        max_split_offset_ms: 0,
        fps_search: false,
        snap_ms,
        lead_in_ms,
        end_adjust_ms: 0,
        ..SyncConfig::default()
    };
    let mut doc = SubtitleDocument::parse(&source, None).unwrap();
    let original = doc.cues.clone();
    assert_eq!(original, cue_list(cues));
    let report = sync_subtitles_with_vad(&mut doc, vad, &config).unwrap();
    assert_eq!(report.cues_end_adjusted, 0);
    assert_eq!(
        report.cues_snapped,
        original
            .iter()
            .zip(&doc.cues)
            .filter(|(before, after)| before.start_ms != after.start_ms)
            .count()
    );
    validate_reported_timing(&original, &doc.cues, format, &report, &config).unwrap();
    doc.cues
}

#[test]
fn recovered_anchors_at_both_snap_window_edges_are_considered() {
    let vad = activity(&[(2000, 3500)], &[1620]);
    for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
        for (start, snap, expected) in [
            (1500, 69, 1500),
            (1500, 70, 1570),
            (1500, 71, 1570),
            (1640, 69, 1640),
            (1640, 70, 1570),
            (1640, 71, 1570),
        ] {
            assert_eq!(
                sync(format, &[(start, 3500)], &vad, snap, 50),
                cue_list(&[(expected, 3500)]),
                "{format:?} start={start} snap={snap}"
            );
        }
    }
}

#[test]
fn confirmed_onsets_use_the_formatted_window_when_anchors_are_empty() {
    let vad = activity(&[(2000, 3500)], &[]);
    for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
        let target = match format {
            SubtitleFormat::Srt => 1999,
            SubtitleFormat::Ass => 1990,
        };
        for start in [target - 350, target + 350] {
            for snap in [349, 350, 351] {
                let expected = if snap < 350 { start } else { target };
                assert_eq!(
                    sync(format, &[(start, 3500)], &vad, snap, 1),
                    cue_list(&[(expected, 3500)]),
                    "{format:?} start={start} snap={snap}"
                );
            }
        }
    }
}

#[test]
fn valid_onset_anchors_need_not_be_sorted_for_candidate_search() {
    let vad = activity(&[(2000, 2050), (2100, 3500)], &[2000, 1720]);
    for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
        assert_eq!(
            sync(format, &[(1600, 3500)], &vad, 100, 50),
            cue_list(&[(1670, 3500)]),
            "{format:?} descending anchors"
        );
    }
}

#[test]
fn expanded_search_preserves_endpoint_and_neighbor_guards() {
    let vad = activity(&[(2000, 3500)], &[1620]);
    for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
        let unit = if format == SubtitleFormat::Srt { 1 } else { 10 };
        for (name, input, snap, expected) in [
            (
                "previous end blocks",
                vec![(500, 1580), (1640, 3500)],
                70,
                vec![(500, 1580), (1640, 3500)],
            ),
            (
                "previous end inclusive",
                vec![(500, 1570), (1640, 3500)],
                70,
                vec![(500, 1570), (1570, 3500)],
            ),
            (
                "joined forward limit",
                vec![(500, 1400), (1480, 3500)],
                90,
                vec![(500, 1400), (1480, 3500)],
            ),
            (
                "less than 500 ms after snap",
                vec![(1500, 2070 - unit)],
                70,
                vec![(1500, 2070 - unit)],
            ),
            (
                "exactly 500 ms after snap",
                vec![(1500, 2070)],
                70,
                vec![(1570, 2070)],
            ),
            (
                "more than 500 ms after snap",
                vec![(1500, 2070 + unit)],
                70,
                vec![(1570, 2070 + unit)],
            ),
            (
                "short cue before snap",
                vec![(1640, 2140 - unit)],
                70,
                vec![(1640, 2140 - unit)],
            ),
            ("disabled", vec![(1500, 3500)], 0, vec![(1500, 3500)]),
            (
                "20 ms forward movement",
                vec![(1550, 3500)],
                350,
                vec![(1550, 3500)],
            ),
            (
                "more than 20 ms forward movement",
                vec![(1550 - unit, 3500)],
                350,
                vec![(1570, 3500)],
            ),
            (
                "small backward movement",
                vec![(1580, 3500)],
                350,
                vec![(1570, 3500)],
            ),
            (
                "already at the anchor",
                vec![(1570, 3500)],
                350,
                vec![(1570, 3500)],
            ),
        ] {
            assert_eq!(
                sync(format, &input, &vad, snap, 50),
                cue_list(&expected),
                "{format:?} {name}"
            );
        }
        assert_eq!(
            sync(
                format,
                &[(100, 1000)],
                &activity(&[(430, 2000)], &[50]),
                100,
                50
            ),
            cue_list(&[(100, 1000)]),
            "{format:?} zero target remains forbidden"
        );
        assert_eq!(
            sync(format, &[(1000, 1900)], &vad, 150, 500),
            cue_list(&[(1000, 1900)]),
            "{format:?} confirmed onset must precede the cue end"
        );
    }
}

#[test]
fn expanded_search_preserves_overlap_scoring_and_intentional_ass_overlap() {
    let vad = activity(&[(1150, 1210), (1800, 2600)], &[]);
    for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
        assert_eq!(
            sync(format, &[(1400, 2600)], &vad, 350, 50),
            cue_list(&[(1750, 2600)]),
            "{format:?} nearer low-overlap candidate must be skipped"
        );
    }
    let input = [(1600, 3500), (1500, 3400)];
    assert_eq!(
        sync(
            SubtitleFormat::Ass,
            &input,
            &activity(&[(2000, 3500)], &[1620]),
            350,
            50,
        ),
        cue_list(&input)
    );
}

#[test]
fn authored_audio_recovers_an_attack_outside_the_confirmation_window() {
    let mut pcm = vec![0; 8 * 16_000];
    let attack = [
        (123.0, 1500.0),
        (246.0, 1200.0),
        (369.0, 500.0),
        (738.0, 450.0),
    ];
    let confirmed = [(400.0, 3500.0), (1200.0, 3500.0), (2400.0, 3500.0)];
    for (start, end, tones) in [
        (2000, 2150, attack.as_slice()),
        (2300, 3400, confirmed.as_slice()),
    ] {
        for (i, sample) in pcm.iter_mut().enumerate().take(end * 16).skip(start * 16) {
            *sample = tones
                .iter()
                .map(|&(hz, gain)| gain * (TAU * hz * i as f64 / 16_000.0).sin())
                .sum::<f64>() as i16;
        }
    }
    let vad = extract_speech_activity(&pcm, 16_000, 10);
    assert_eq!(vad.onset_anchors_ms, [2000]);
    assert!(vad.segments[0].start_ms > 1850 + 350 + 50);
    for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
        for (start, snap, expected) in [
            (1850, 350, 1950),
            (1880, 350, 1950),
            (1890, 350, 1950),
            (1850, 400, 1950),
            (2230, 350, 1950),
            (1500, 350, 1500),
        ] {
            assert_eq!(
                sync(format, &[(start, 3400)], &vad, snap, 50),
                cue_list(&[(expected, 3400)]),
                "{format:?} start={start} snap={snap}"
            );
        }
    }
}
