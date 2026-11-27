use subsync_rs::align::validate_reported_timing;
use subsync_rs::vad::{TimeSpan, VadResult};
use subsync_rs::{SubtitleDocument, SubtitleFormat, SyncConfig, sync_subtitles_with_vad};

fn source(format: SubtitleFormat, cues: &[(u64, u64)]) -> String {
    let time = |ms: u64| match format {
        SubtitleFormat::Srt => format!("00:00:{:02},{:03}", ms / 1000, ms % 1000),
        SubtitleFormat::Ass => format!("0:00:{:02}.{:02}", ms / 1000, ms % 1000 / 10),
    };
    let mut text = if format == SubtitleFormat::Ass {
        "[Events]\nFormat: Start, End, Text\n".to_owned()
    } else {
        String::new()
    };
    for (i, &(start, end)) in cues.iter().enumerate() {
        let (start, end) = (time(start), time(end));
        if format == SubtitleFormat::Ass {
            text.push_str(&format!("Dialogue: {start},{end},Authored {i}\n"));
        } else {
            text.push_str(&format!("{}\n{start} --> {end}\nAuthored {i}\n\n", i + 1));
        }
    }
    text
}

fn check(
    format: SubtitleFormat,
    cues: &[(u64, u64)],
    speech: (u64, u64),
    duration: u64,
    limit: u64,
    expected_ends: &[u64],
) {
    let mut activity = vec![0_u8; duration.div_ceil(10) as usize];
    activity[speech.0 as usize / 10..speech.1.div_ceil(10) as usize].fill(1);
    let vad = VadResult {
        raw_segments: Vec::new(),
        onset_anchors_ms: Vec::new(),
        frame_ms: 10,
        duration_ms: duration,
        probability: activity.iter().map(|&value| f32::from(value)).collect(),
        activity,
        segments: vec![TimeSpan {
            start_ms: speech.0,
            end_ms: speech.1,
        }],
    };
    let config = SyncConfig {
        max_offset_ms: 0,
        max_split_offset_ms: 0,
        fps_search: false,
        snap_ms: 0,
        end_adjust_ms: limit,
        ..SyncConfig::default()
    };
    let mut doc = SubtitleDocument::parse(&source(format, cues), None).unwrap();
    let original = doc.cues.clone();
    let report = sync_subtitles_with_vad(&mut doc, &vad, &config).unwrap();
    assert_eq!(
        doc.cues.iter().map(|cue| cue.end_ms).collect::<Vec<_>>(),
        expected_ends,
        "format={format:?}, speech={speech:?}, duration={duration}, limit={limit}"
    );
    assert!(doc.cues.iter().zip(&original).all(|(after, before)| {
        after.start_ms == before.start_ms
            && after.start_ms < after.end_ms
            && after.end_ms.abs_diff(before.end_ms) <= limit
    }));
    assert_eq!(
        report.cues_end_adjusted,
        original
            .iter()
            .zip(&doc.cues)
            .filter(|(before, after)| before.end_ms != after.end_ms)
            .count()
    );
    assert_eq!(report.cues_snapped, 0);
    validate_reported_timing(&original, &doc.cues, format, &report, &config).unwrap();
}

#[test]
fn ongoing_speech_extension_is_bounded_without_requiring_full_coverage() {
    for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
        for speech_end in [2599, 2600, 2601, 3100] {
            check(
                format,
                &[(1000, 2000)],
                (1000, speech_end),
                8000,
                600,
                &[2600],
            );
        }
        let unit = if format == SubtitleFormat::Srt { 1 } else { 10 };
        for limit in [0, 1, 9, 10, 140, 600] {
            check(
                format,
                &[(1000, 2000)],
                (1000, 3100),
                8000,
                limit,
                &[2000 + limit / unit * unit],
            );
        }
    }
}

#[test]
fn extension_keeps_joined_chains_neighbors_and_audio_end_bounded() {
    for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
        check(
            format,
            &[(1000, 2000), (2030, 3100), (3130, 4000)],
            (1000, 5000),
            8000,
            600,
            &[2030, 3130, 4600],
        );
        check(
            format,
            &[(1000, 2000), (2400, 3400)],
            (1000, 3100),
            8000,
            600,
            &[2380, 3400],
        );
        check(format, &[(1000, 2000)], (1000, 2500), 2500, 600, &[2500]);
        check(format, &[(1000, 2000)], (1000, 3100), 3100, 600, &[2600]);
        check(format, &[(1000, 2000)], (1000, 3100), 3100, 1200, &[3100]);
        check(format, &[(2300, 2450)], (2300, 2500), 2500, 600, &[2450]);
    }
    check(
        SubtitleFormat::Ass,
        &[(4000, 4500), (1800, 3100), (1000, 2000)],
        (1000, 5000),
        8000,
        600,
        &[5100, 3100, 2000],
    );
}

#[test]
fn end_extension_requires_ongoing_speech_and_preserves_existing_thresholds() {
    for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
        for speech in [(100, 900), (2000, 3100), (2001, 3100), (1000, 2140)] {
            check(format, &[(1000, 2000)], speech, 8000, 600, &[2000]);
        }
        let expected = if format == SubtitleFormat::Srt {
            2391
        } else {
            2390
        };
        check(
            format,
            &[(1000, 2000)],
            (1000, 2141),
            8000,
            600,
            &[expected],
        );
        check(format, &[(1000, 1400)], (1000, 1300), 8000, 600, &[1900]);
        check(format, &[(1000, 3600)], (1000, 2100), 8000, 600, &[3000]);
    }
}
