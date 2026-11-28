use std::{
    f64::consts::TAU,
    time::{SystemTime, UNIX_EPOCH},
};
use subsync_rs::{
    SubtitleCue, SubtitleDocument, SubtitleFormat, SyncConfig,
    align::validate_reported_timing,
    sync_subtitles_with_vad,
    vad::{VadResult, extract_speech_activity},
};

const RATE: usize = 16_000;
const FORMATS: [SubtitleFormat; 2] = [SubtitleFormat::Srt, SubtitleFormat::Ass];

fn log(level: &str, message: &str) {
    assert!(matches!(level, "INFO" | "WARNING" | "ERROR" | "DEBUG"));
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    eprintln!(
        "{}.{:03}Z [{level}] {message}",
        timestamp.as_secs(),
        timestamp.subsec_millis()
    );
}

fn detect(spans: &[(u64, u64, f64)]) -> VadResult {
    let mut pcm = vec![0_i16; 15 * RATE];
    for &(start, end, amplitude) in spans {
        for (index, sample) in pcm
            .iter_mut()
            .enumerate()
            .take(end as usize * 16)
            .skip(start as usize * 16)
        {
            *sample = [400.0, 1200.0, 2400.0]
                .iter()
                .map(|hz| amplitude * (TAU * hz * index as f64 / RATE as f64).sin())
                .sum::<f64>() as i16;
        }
    }
    let vad = extract_speech_activity(&pcm, RATE as u32, 10);
    log(
        "DEBUG",
        &format!("PCM speech spans={spans:?}; primary={:?}", vad.segments),
    );
    vad
}

fn configured(endpoints: bool) -> SyncConfig {
    let mut config = SyncConfig {
        max_offset_ms: 0,
        max_split_offset_ms: 1500,
        fps_search: false,
        ..SyncConfig::default()
    };
    if !endpoints {
        config.snap_ms = 0;
        config.end_adjust_ms = 0;
    }
    config
}

fn synchronize(
    format: SubtitleFormat,
    times: &[(u64, u64)],
    vad: &VadResult,
    config: &SyncConfig,
) -> Vec<SubtitleCue> {
    log(
        "INFO",
        &format!("Synchronizing {format:?} cues={times:?}; config={config:?}"),
    );
    let time = |ms: u64| match format {
        SubtitleFormat::Srt => format!("00:00:{:02},{:03}", ms / 1000, ms % 1000),
        SubtitleFormat::Ass => format!("0:00:{:02}.{:02}", ms / 1000, ms % 1000 / 10),
    };
    let mut source = match format {
        SubtitleFormat::Srt => String::new(),
        SubtitleFormat::Ass => "[Events]\nFormat: Start, End, Text\n".to_owned(),
    };
    for (index, &(start, end)) in times.iter().enumerate() {
        let (start, end) = (time(start), time(end));
        match format {
            SubtitleFormat::Srt => source.push_str(&format!(
                "{}\n{start} --> {end}\nAuthored cue {index}: English 123.\n\n",
                index + 1
            )),
            SubtitleFormat::Ass => source.push_str(&format!(
                "Dialogue: {start},{end},Authored cue {index}: English 123.\n"
            )),
        }
    }
    let original = SubtitleDocument::parse(&source, None).unwrap();
    let mut output = SubtitleDocument::parse(&source, None).unwrap();
    let report = sync_subtitles_with_vad(&mut output, vad, config).unwrap();
    assert_eq!(report.global_offset_ms, 0);
    assert_eq!(report.fps_ratio, 1.0);
    validate_reported_timing(&original.cues, &output.cues, format, &report, config).unwrap();
    let serialized = SubtitleDocument::parse(&output.serialize().unwrap(), None).unwrap();
    assert_eq!(serialized.cues, output.cues);
    assert!(original.has_same_untimed_content(&serialized));
    assert!(
        output
            .cues
            .windows(2)
            .all(|pair| pair[0].end_ms <= pair[1].start_ms)
    );
    log(
        "DEBUG",
        &format!("Output={:?}; report={report:?}", output.cues),
    );
    output.cues
}

fn near(cue: SubtitleCue, start: u64, context: &str) {
    let recovered = cue.start_ms.abs_diff(start) <= 100;
    log(
        if recovered { "INFO" } else { "ERROR" },
        &format!("{context}: got={cue:?}; expected onset within 100 ms of {start}"),
    );
    assert!(recovered, "{context}: {cue:?}");
}

fn late_single(duration: u64) {
    let vad = detect(&[
        (1000, 2000, 3500.0),
        (5000, 5000 + duration, 3500.0),
        (11000, 12000, 3500.0),
    ]);
    for format in FORMATS {
        for endpoints in [true, false] {
            let output = synchronize(
                format,
                &[(1000, 2000), (5550, 5550 + duration), (11000, 12000)],
                &vad,
                &configured(endpoints),
            );
            near(
                output[1],
                5000,
                "late cue must recover the onset of its own speech island",
            );
        }
    }
}

#[test]
fn a_late_long_cue_with_high_existing_coverage_recovers() {
    // The cue overlaps most of its true island but misses the first 550 ms of speech.
    late_single(2000);
}

#[test]
fn a_late_short_cue_with_vad_tail_coverage_recovers() {
    // The VAD trail makes even this shorter cue exceed the covered-window threshold.
    late_single(1380);
}

#[test]
fn a_late_joined_chain_recovers_without_opening_a_gap() {
    let vad = detect(&[
        (1000, 2000, 3500.0),
        (5000, 6000, 3500.0),
        (6030, 7030, 3500.0),
        (11000, 12000, 3500.0),
    ]);
    for format in FORMATS {
        for endpoints in [true, false] {
            let output = synchronize(
                format,
                &[(1000, 2000), (5550, 6550), (6580, 7580), (11000, 12000)],
                &vad,
                &configured(endpoints),
            );
            near(output[1], 5000, "joined first cue");
            near(output[2], 6030, "joined second cue");
            assert!(output[2].start_ms.saturating_sub(output[1].end_ms) <= 110);
        }
    }
}

#[test]
fn a_zero_local_limit_preserves_the_late_timing() {
    let vad = detect(&[
        (1000, 2000, 3500.0),
        (5000, 7000, 3500.0),
        (11000, 12000, 3500.0),
    ]);
    log(
        "WARNING",
        "Local refinement is intentionally disabled for this control",
    );
    for format in FORMATS {
        for endpoints in [true, false] {
            let config = SyncConfig {
                max_split_offset_ms: 0,
                ..configured(endpoints)
            };
            let output = synchronize(
                format,
                &[(1000, 2000), (5550, 7550), (11000, 12000)],
                &vad,
                &config,
            );
            assert_eq!(
                output[1],
                SubtitleCue {
                    start_ms: 5550,
                    end_ms: 7550
                }
            );
        }
    }
}

#[test]
fn an_early_long_cue_still_recovers() {
    let vad = detect(&[
        (1000, 2000, 3500.0),
        (5000, 7000, 3500.0),
        (11000, 12000, 3500.0),
    ]);
    for format in FORMATS {
        for endpoints in [true, false] {
            let output = synchronize(
                format,
                &[(1000, 2000), (4450, 6450), (11000, 12000)],
                &vad,
                &configured(endpoints),
            );
            near(output[1], 5000, "early cue control");
        }
    }
}

#[test]
fn a_continuous_wall_with_an_earlier_louder_region_stays_fixed() {
    let vad = detect(&[(500, 7500, 100.0), (1000, 3000, 3500.0)]);
    assert!(
        vad.segments
            .iter()
            .any(|span| span.start_ms <= 600 && span.end_ms >= 7500)
    );
    for format in FORMATS {
        let output = synchronize(format, &[(3500, 5800)], &vad, &configured(false));
        assert_eq!(
            output[0],
            SubtitleCue {
                start_ms: 3500,
                end_ms: 5800
            }
        );
    }
}

#[test]
fn an_anchored_cue_does_not_move_to_a_louder_neighbor() {
    let vad = detect(&[
        (300, 1400, 3500.0),
        (2020, 2760, 100.0),
        (3100, 4480, 5000.0),
        (6000, 7000, 3500.0),
    ]);
    assert!(
        vad.segments
            .iter()
            .any(|span| span.start_ms.abs_diff(2020) <= 20)
    );
    // Reading tails keep the outer anchors stable despite the detector's 100 ms hangover.
    for format in FORMATS {
        let output = synchronize(
            format,
            &[(300, 1550), (2000, 3300), (6000, 7150)],
            &vad,
            &configured(false),
        );
        assert!(output[1].start_ms.abs_diff(2000) <= 80, "{output:?}");
    }
}
