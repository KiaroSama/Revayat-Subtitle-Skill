use std::f64::consts::TAU;
use std::time::{SystemTime, UNIX_EPOCH};
use subsync_rs::align::validate_reported_timing;
use subsync_rs::vad::{VadResult, extract_speech_activity};
use subsync_rs::{
    SubtitleCue, SubtitleDocument, SubtitleFormat, SyncConfig, sync_subtitles_with_vad,
};

const CONFIRMED: &[(f64, f64)] = &[(400.0, 3500.0), (1200.0, 3500.0), (2400.0, 3500.0)];
const FRICATIVE: &[(f64, f64)] = &[(5200.0, 1400.0), (6400.0, 1000.0)];
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

fn log(level: &str, message: impl std::fmt::Display) {
    assert!(matches!(level, "INFO" | "WARNING" | "ERROR" | "DEBUG"));
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    eprintln!("timestamp_ms={timestamp} [{level}] {message}");
}

fn detect(rate: usize, parts: &[ToneSpan<'_>]) -> VadResult {
    let mut pcm = vec![0_i16; 7 * rate];
    for &(start, end, tones) in parts {
        for (i, sample) in pcm
            .iter_mut()
            .enumerate()
            .take(end * rate / 1000)
            .skip(start * rate / 1000)
        {
            *sample = tones
                .iter()
                .map(|&(hz, gain)| gain * (TAU * hz * i as f64 / rate as f64).sin())
                .sum::<f64>() as i16;
        }
    }
    log(
        "DEBUG",
        format_args!("Extracting authored waveform at {rate} Hz"),
    );
    extract_speech_activity(&pcm, rate as u32, 10)
}

fn fixed_config() -> SyncConfig {
    SyncConfig {
        max_offset_ms: 0,
        max_split_offset_ms: 0,
        fps_search: false,
        end_adjust_ms: 0,
        ..SyncConfig::default()
    }
}

fn cue_list(cues: &[(u64, u64)]) -> Vec<SubtitleCue> {
    cues.iter()
        .map(|&(start_ms, end_ms)| SubtitleCue { start_ms, end_ms })
        .collect()
}

fn assert_sync(
    format: SubtitleFormat,
    vad: &VadResult,
    input: &[(u64, u64)],
    expected: &[(u64, u64)],
    config: &SyncConfig,
) {
    let time = |ms: u64| match format {
        SubtitleFormat::Srt => format!("00:00:{:02},{:03}", ms / 1000, ms % 1000),
        SubtitleFormat::Ass => format!("0:00:{:02}.{:02}", ms / 1000, ms % 1000 / 10),
    };
    let mut source = match format {
        SubtitleFormat::Srt => String::new(),
        SubtitleFormat::Ass => "[Events]\nFormat: Start, End, Text\n".to_owned(),
    };
    for (i, &(start, end)) in input.iter().enumerate() {
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
    let mut doc = SubtitleDocument::parse(&source, None).unwrap();
    let original = doc.cues.clone();
    assert_eq!(original, cue_list(input));
    let report = sync_subtitles_with_vad(&mut doc, vad, config).unwrap();
    if doc.cues != cue_list(expected) {
        log(
            "ERROR",
            format_args!(
                "{format:?} input={input:?}, actual={:?}, expected={expected:?}",
                doc.cues
            ),
        );
    }
    assert_eq!(doc.cues, cue_list(expected), "{format:?} input={input:?}");
    assert_eq!(report.global_offset_ms, 0);
    assert_eq!(report.fps_ratio, 1.0);
    assert!(report.split_segments.iter().all(|part| part.shift_ms == 0));
    assert_eq!(report.cues_end_adjusted, 0);
    assert_eq!(
        report.cues_snapped,
        original
            .iter()
            .zip(&doc.cues)
            .filter(|(before, after)| before.start_ms != after.start_ms)
            .count()
    );
    validate_reported_timing(&original, &doc.cues, format, &report, config).unwrap();
    let serialized = doc.serialize().unwrap();
    assert_eq!(
        SubtitleDocument::parse(&serialized, None).unwrap().cues,
        doc.cues
    );
    log(
        "INFO",
        format_args!("Validated {format:?} timing and replay for {input:?}"),
    );
}

#[test]
fn recovered_fricative_is_used_at_and_after_primary_confirmation() {
    // The attack is intentionally unvoiced. Its validated onset must survive cue snapping.
    for rate in [16_000, 44_100, 48_000] {
        let vad = detect(rate, &[(2050, 2300, FRICATIVE), (2300, 3400, CONFIRMED)]);
        assert_eq!(vad.segments.len(), 1);
        assert_eq!(vad.segments[0].start_ms, 2300);
        assert_eq!(vad.onset_anchors_ms, [2050]);
        assert!(
            vad.voiced_activity[205..229]
                .iter()
                .all(|&value| value == 0)
        );
        for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
            for start in [2200, 2290, 2300, 2350] {
                assert_sync(
                    format,
                    &vad,
                    &[(start, 3400)],
                    &[(2000, 3400)],
                    &fixed_config(),
                );
            }
        }
    }
}

#[test]
fn recovered_quiet_and_deep_voiced_attacks_survive_probability_guards() {
    for tones in [QUIET, LOUD] {
        let vad = detect(16_000, &[(2000, 2150, tones), (2300, 3400, CONFIRMED)]);
        assert_eq!(vad.segments.len(), 1);
        assert_eq!(vad.segments[0].start_ms, 2290);
        assert_eq!(vad.onset_anchors_ms, [2000]);
        assert!(vad.voiced_activity[200..215].contains(&1));
        for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
            for start in [2200, 2290, 2300] {
                assert_sync(
                    format,
                    &vad,
                    &[(start, 3400)],
                    &[(1950, 3400)],
                    &fixed_config(),
                );
            }
            // A valid recovered onset still cannot exceed the default snap distance.
            assert_sync(
                format,
                &vad,
                &[(2310, 3400)],
                &[(2240, 3400)],
                &fixed_config(),
            );
        }
    }
}

#[test]
fn recovered_onsets_keep_exact_radius_neighbor_and_reading_bounds() {
    let vad = detect(16_000, &[(2050, 2300, FRICATIVE), (2300, 3400, CONFIRMED)]);
    for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
        for (radius, expected) in [(299, 2250), (300, 2000), (301, 2000)] {
            let config = SyncConfig {
                snap_ms: radius,
                ..fixed_config()
            };
            assert_sync(format, &vad, &[(2300, 3400)], &[(expected, 3400)], &config);
        }
        log(
            "WARNING",
            "Checking that recovered speech cannot cross an independent previous cue.",
        );
        assert_sync(
            format,
            &vad,
            &[(1500, 2010), (2300, 3400)],
            &[(1500, 2010), (2250, 3400)],
            &fixed_config(),
        );
        assert_sync(
            format,
            &vad,
            &[(1500, 2000), (2300, 3400)],
            &[(1500, 2000), (2000, 3400)],
            &fixed_config(),
        );
        let unit = if format == SubtitleFormat::Srt { 1 } else { 10 };
        assert_sync(
            format,
            &vad,
            &[(1800, 2500 - unit)],
            &[(1800, 2500 - unit)],
            &fixed_config(),
        );
        assert_sync(
            format,
            &vad,
            &[(1800, 2500)],
            &[(2000, 2500)],
            &fixed_config(),
        );
        assert_sync(
            format,
            &vad,
            &[(2300, 2790)],
            &[(2300, 2790)],
            &fixed_config(),
        );
        let disabled = SyncConfig {
            snap_ms: 0,
            ..fixed_config()
        };
        assert_sync(format, &vad, &[(2300, 3400)], &[(2300, 3400)], &disabled);
    }
    assert_sync(
        SubtitleFormat::Ass,
        &vad,
        &[(1500, 2500), (2300, 3400)],
        &[(1500, 2500), (2300, 3400)],
        &fixed_config(),
    );
}

#[test]
fn false_fricative_boundaries_and_loud_background_keep_confirmation() {
    let background = [(70.0, 3300.0)];
    let cases: &[&[ToneSpan<'_>]] = &[
        &[(500, 2300, FRICATIVE), (2300, 3400, CONFIRMED)],
        &[(1910, 2300, FRICATIVE), (2300, 3400, CONFIRMED)],
        &[
            (1800, 2050, &background),
            (2050, 2300, FRICATIVE),
            (2300, 3400, CONFIRMED),
        ],
        &[
            (1000, 1900, CONFIRMED),
            (1900, 2300, FRICATIVE),
            (2300, 3400, CONFIRMED),
        ],
        &[(2050, 2200, FRICATIVE), (2300, 3400, CONFIRMED)],
    ];
    for parts in cases {
        let vad = detect(16_000, parts);
        let confirmed = vad.segments.last().unwrap().start_ms;
        assert_eq!(vad.onset_anchors_ms.last().copied(), Some(confirmed));
        for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
            for start in [2200, 2300] {
                assert_sync(
                    format,
                    &vad,
                    &[(start, 3400)],
                    &[(confirmed - 50, 3400)],
                    &fixed_config(),
                );
            }
        }
    }
}

#[test]
fn real_fricative_attacks_at_and_inside_the_lookback_survive() {
    for attack in [1920, 1930] {
        let vad = detect(
            16_000,
            &[(attack, 2300, FRICATIVE), (2300, 3400, CONFIRMED)],
        );
        assert_eq!(vad.segments.len(), 1);
        assert_eq!(vad.segments[0].start_ms, 2300);
        assert_eq!(vad.onset_anchors_ms, [attack as u64]);
        let config = SyncConfig {
            snap_ms: 450,
            ..fixed_config()
        };
        for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
            for start in [2200, 2300] {
                assert_sync(
                    format,
                    &vad,
                    &[(start, 3400)],
                    &[(attack as u64 - 50, 3400)],
                    &config,
                );
            }
        }
    }
}
