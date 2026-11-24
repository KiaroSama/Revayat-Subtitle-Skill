use std::f64::consts::TAU;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use subsync_rs::audio::{read_wav_pcm16, write_wav_pcm16};
use subsync_rs::formats::{SubtitleDocument, SubtitleFormat};
use subsync_rs::vad::{TimeSpan, VadResult, extract_speech_activity};
use subsync_rs::{SyncConfig, SyncError, sync_file_to_wav, sync_subtitles_with_vad};

const RATE: u32 = 16_000;
const SPEECH: [(u64, u64); 16] = [
    (1520, 2470),
    (3110, 3890),
    (4990, 6370),
    (6780, 7390),
    (8020, 9150),
    (10590, 12420),
    (13160, 13840),
    (14760, 16040),
    (24530, 25640),
    (27200, 28080),
    (28880, 29590),
    (31230, 32980),
    (33740, 34650),
    (35980, 37080),
    (37680, 38270),
    (39300, 40680),
];

fn samples(intervals: &[(u64, u64)]) -> Vec<i16> {
    let end = intervals.last().map_or(1000, |span| span.1 + 3000);
    (0..end * 16)
        .map(|i| {
            if intervals.iter().any(|&(s, e)| s * 16 <= i && i < e * 16) {
                [400.0, 1200.0, 2400.0]
                    .into_iter()
                    .map(|hz| 3500.0 * (TAU * hz * i as f64 / f64::from(RATE)).sin())
                    .sum::<f64>() as i16
            } else {
                ((i * 4093) % 131) as i16 - 65
            }
        })
        .collect()
}

fn srt_time(ms: u64) -> String {
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

fn ass_time(ms: u64) -> String {
    format!(
        "{}:{:02}:{:02}.{:02}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000 / 10
    )
}

fn srt(intervals: impl IntoIterator<Item = (u64, u64)>) -> String {
    use std::fmt::Write as _;
    let mut text = String::new();
    for (i, (s, e)) in intervals.into_iter().enumerate() {
        writeln!(
            text,
            "{}\n{} --> {}\n<i>Subtitle</i>\nSecond line\n",
            i + 1,
            srt_time(s),
            srt_time(e)
        )
        .unwrap();
    }
    text
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "subsync test {} {}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("create test directory: {e}"),
            }
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn assert_times(doc: &SubtitleDocument, expected: &[(u64, u64)], tolerance_ms: u64) {
    assert_eq!(doc.cues.len(), expected.len());
    for (i, (cue, &(s, e))) in doc.cues.iter().zip(expected).enumerate() {
        assert!(
            cue.start_ms.abs_diff(s) <= tolerance_ms,
            "cue {} starts at {}, expected {s}",
            i + 1,
            cue.start_ms
        );
        assert!(
            cue.end_ms.abs_diff(e) <= tolerance_ms,
            "cue {} ends at {}, expected {e}",
            i + 1,
            cue.end_ms
        );
    }
}

#[test]
fn shifts_srt_back_by_3200_ms_using_wav() {
    let scratch = Scratch::new();
    let wav = scratch.0.join("audio.wav");
    let original = scratch.0.join("original.srt");
    let aligned = scratch.0.join("aligned.srt");
    let pcm = samples(&SPEECH);
    write_wav_pcm16(&wav, RATE, &pcm).unwrap();
    assert_eq!(read_wav_pcm16(&wav).unwrap(), (RATE, pcm));
    fs::write(&original, srt(SPEECH.map(|(s, e)| (s + 3200, e + 3200)))).unwrap();
    let report = sync_file_to_wav(&wav, &original, &aligned, &SyncConfig::default()).unwrap();
    assert!(report.global_offset_ms.abs_diff(-3200) <= 100, "{report:?}");
    assert!(report.overlap_score_after - report.overlap_score_before > 0.35);
    let result = fs::read_to_string(&aligned).unwrap();
    assert!(result.contains("<i>Subtitle</i>\nSecond line"));
    assert_times(
        &SubtitleDocument::parse(&result, Some(&aligned)).unwrap(),
        &SPEECH,
        100,
    );
}

#[test]
fn shifts_utf8_ass_forward_and_preserves_non_dialogue_bytes() {
    let expected = SPEECH.map(|(s, e)| (s + 2400, e + 2400));
    let vad = extract_speech_activity(&samples(&expected), RATE, 10);
    let header = "[Script Info]\r\nTitle: نمونه\r\n[V4+ Styles]\r\nStyle: Default\r\n[Fonts]\r\nfontname: test.ttf\r\nabcdef\r\n[Graphics]\r\nfilename: test.bmp\r\nABCDEF\r\n[Events]\r\nFormat: Layer, End, Style, Name, MarginL, MarginR, MarginV, Effect, Start, Text\r\nComment: 0,0:00:00.10,Default,,0,0,0,,0:00:00.00,یادداشت\r\n";
    let mut original = format!("\u{feff}{header}");
    for (s, e) in SPEECH {
        original.push_str(&format!(
            "Dialogue: 2,{},Default,A,0010,0020,0030,fx,{},{{\\i1}}{{\\an8}}سلام، دنیا\r\n",
            ass_time(e),
            ass_time(s)
        ));
    }
    let mut doc = SubtitleDocument::parse(&original, None).unwrap();
    assert_eq!(doc.format, SubtitleFormat::Ass);
    let report = sync_subtitles_with_vad(&mut doc, &vad, &SyncConfig::default()).unwrap();
    assert!(report.global_offset_ms.abs_diff(2400) <= 100, "{report:?}");
    let output = doc.serialize().unwrap();
    assert!(output.starts_with(header));
    assert_eq!(
        output.matches(",Default,A,0010,0020,0030,fx,").count(),
        SPEECH.len()
    );
    assert_eq!(
        output.matches(r"{\i1}{\an8}سلام، دنیا").count(),
        SPEECH.len()
    );
    assert_times(
        &SubtitleDocument::parse(&output, None).unwrap(),
        &expected,
        100,
    );
}

#[test]
fn detects_pal_to_ntsc_drift() {
    let ratio = 25.0 / 23.976;
    let vad = extract_speech_activity(&samples(&SPEECH), RATE, 10);
    let scaled = SPEECH.map(|(s, e)| {
        (
            (s as f64 * ratio).round() as u64,
            (e as f64 * ratio).round() as u64,
        )
    });
    let mut doc = SubtitleDocument::parse(&srt(scaled), None).unwrap();
    let report = sync_subtitles_with_vad(&mut doc, &vad, &SyncConfig::default()).unwrap();
    assert!((report.fps_ratio - 1.0 / ratio).abs() < 1e-6, "{report:?}");
    assert_times(&doc, &SPEECH, 100);
}

#[test]
fn detects_a_five_second_edit_and_aligns_both_halves() {
    let vad = extract_speech_activity(&samples(&SPEECH), RATE, 10);
    let mut shifted = SPEECH;
    for (s, e) in &mut shifted[8..] {
        *s += 5000;
        *e += 5000;
    }
    let mut doc = SubtitleDocument::parse(&srt(shifted), None).unwrap();
    let config = SyncConfig {
        fps_search: false,
        ..SyncConfig::default()
    };
    let report = sync_subtitles_with_vad(&mut doc, &vad, &config).unwrap();
    assert_eq!(report.split_segments.len(), 2, "{report:?}");
    assert_eq!(report.split_segments[0].end_cue, 8);
    assert!(report.split_segments[0].shift_ms.abs_diff(0) <= 150);
    assert!(report.split_segments[1].shift_ms.abs_diff(-5000) <= 150);
    assert_times(&doc, &SPEECH, 150);
}

#[test]
fn retimes_overlapping_srt_cues_to_positive_nonoverlapping_spans() {
    let vad = extract_speech_activity(&samples(&SPEECH), RATE, 10);
    let mut doc =
        SubtitleDocument::parse(&srt([(1520, 2950), (2500, 3890), (4890, 6340)]), None).unwrap();
    let config = SyncConfig {
        fps_search: false,
        max_offset_ms: 0,
        max_split_offset_ms: 0,
        ..SyncConfig::default()
    };
    sync_subtitles_with_vad(&mut doc, &vad, &config).unwrap();
    assert!(doc.cues.iter().all(|cue| cue.start_ms < cue.end_ms));
    assert!(
        doc.cues
            .windows(2)
            .all(|pair| pair[0].end_ms <= pair[1].start_ms)
    );
    assert!(doc.cues[2].end_ms - doc.cues[2].start_ms >= 500);
}

#[test]
fn refuses_invalid_subtitles_and_empty_speech() {
    assert!(SubtitleDocument::parse("\u{feff}\r\n", None).is_err());
    assert!(SubtitleDocument::parse("1\n00:00:10,900 --> 00:00:01,000\ntext\n", None).is_err());
    let mut doc = SubtitleDocument::parse(&srt([(10, 20)]), None).unwrap();
    let vad = extract_speech_activity(&[], RATE, 10);
    assert!(matches!(
        sync_subtitles_with_vad(&mut doc, &vad, &SyncConfig::default()),
        Err(SyncError::NoSpeech)
    ));
}

#[test]
fn refuses_continuous_activity_without_timing_anchors() {
    let pcm: Vec<_> = (0..10 * RATE)
        .map(|i| (9000.0 * (TAU * 400.0 * f64::from(i) / f64::from(RATE)).sin()) as i16)
        .collect();
    let vad = extract_speech_activity(&pcm, RATE, 10);
    assert!(vad.activity.iter().all(|&active| active == 1));
    let original = srt([(1000, 1800), (2700, 3500), (4500, 6100)]);
    let mut doc = SubtitleDocument::parse(&original, None).unwrap();
    let result = sync_subtitles_with_vad(&mut doc, &vad, &SyncConfig::default());
    assert!(
        result.is_err(),
        "constant activity produced an alignment: {result:?}"
    );
    assert_eq!(doc.serialize().unwrap(), original);
}

#[test]
fn refuses_seeded_broadband_noise_without_changing_subtitles() {
    let mut state = 127_u32;
    let pcm: Vec<_> = (0..5 * RATE)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((state >> 16) as i16) / 2
        })
        .collect();
    let vad = extract_speech_activity(&pcm, RATE, 10);
    let original = srt([(1000, 1800), (2700, 3500)]);
    let mut doc = SubtitleDocument::parse(&original, None).unwrap();
    assert!(matches!(
        sync_subtitles_with_vad(&mut doc, &vad, &SyncConfig::default()),
        Err(SyncError::NoSpeech)
    ));
    assert_eq!(doc.serialize().unwrap(), original);
}

#[test]
fn aligns_twelve_edited_sections_across_a_twelve_minute_timeline() {
    let shifts = [
        0_i64, 5000, -3000, 2000, -5000, 1000, 5000, -1000, 4000, -4000, 0, 3000,
    ];
    let mut expected = Vec::new();
    let mut original = Vec::new();
    for (block, shift) in shifts.into_iter().enumerate() {
        for (start, end) in SPEECH {
            let base = 5000 + block as u64 * 60_000;
            let (start, end) = (base + start, base + end);
            expected.push((start, end));
            original.push((
                start.saturating_add_signed(shift),
                end.saturating_add_signed(shift),
            ));
        }
    }
    let duration_ms = 12 * 60_000;
    let mut activity = vec![0; (duration_ms / 10) as usize];
    for &(start, end) in &expected {
        activity[start as usize / 10..end as usize / 10].fill(1);
    }
    let vad = VadResult {
        frame_ms: 10,
        duration_ms,
        probability: activity.iter().map(|&active| f32::from(active)).collect(),
        activity,
        segments: expected
            .iter()
            .map(|&(start_ms, end_ms)| TimeSpan { start_ms, end_ms })
            .collect(),
    };
    let mut doc = SubtitleDocument::parse(&srt(original), None).unwrap();
    let config = SyncConfig {
        fps_search: false,
        snap_ms: 0,
        ..SyncConfig::default()
    };
    let report = sync_subtitles_with_vad(&mut doc, &vad, &config).unwrap();
    assert_times(&doc, &expected, 0);
    assert_eq!(report.split_segments.len(), shifts.len());
    assert_eq!(report.split_segments[0].start_cue, 1);
    assert_eq!(
        report.split_segments.last().unwrap().end_cue,
        expected.len()
    );
    assert!(
        report
            .split_segments
            .windows(2)
            .all(|pair| pair[0].end_cue + 1 == pair[1].start_cue)
    );
    assert_eq!(report.overlap_score_after, 1.0);
}
