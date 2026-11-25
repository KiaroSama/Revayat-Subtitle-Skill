#![forbid(unsafe_code)]

pub mod align;
pub mod audio;
mod error;
pub mod formats;
mod input;
mod publication;
pub mod vad;

pub use error::SyncError;
pub use formats::{SubtitleCue, SubtitleDocument, SubtitleFormat};
pub use input::{MAX_SUBTITLE_BYTES, open_bounded_regular_file};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use vad::{VadResult, extract_speech_activity_cancellable};

const FRAME_MS: u32 = 10;
static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Request cooperative cancellation without installing a process-wide signal handler.
pub fn request_cancel() {
    CANCEL_REQUESTED.store(true, Ordering::Relaxed);
}

/// Stop before additional work or publication after the CLI requests cancellation.
pub fn check_cancelled() -> Result<(), SyncError> {
    if CANCEL_REQUESTED.load(Ordering::Relaxed) {
        Err(SyncError::Cancelled)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SyncConfig {
    pub max_offset_ms: i64,
    pub max_split_offset_ms: i64,
    pub split_penalty: f64,
    pub snap_ms: u64,
    pub fps_search: bool,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            max_offset_ms: 60_000,
            max_split_offset_ms: 15_000,
            split_penalty: 7.0,
            snap_ms: 350,
            fps_search: true,
        }
    }
}

impl SyncConfig {
    pub fn validate(&self) -> Result<(), SyncError> {
        if !(0..=600_000).contains(&self.max_offset_ms)
            || !(0..=60_000).contains(&self.max_split_offset_ms)
        {
            return Err(SyncError::Config(
                "offset bounds must be within 0..=600000 ms globally and 0..=60000 ms locally"
                    .into(),
            ));
        }
        if !self.split_penalty.is_finite() || self.split_penalty < 0.0 || self.snap_ms > 5000 {
            return Err(SyncError::Config(
                "split penalty must be finite and nonnegative; snapping must be within 0..=5000 ms"
                    .into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct SplitSegment {
    pub start_cue: usize,
    pub end_cue: usize,
    pub shift_ms: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SyncReport {
    pub video_path: Option<String>,
    pub wav_path: Option<String>,
    pub subtitle_in: Option<String>,
    pub subtitle_out: Option<String>,
    pub audio_duration_ms: u64,
    pub speech_segments_count: usize,
    pub global_offset_ms: i64,
    pub fps_ratio: f64,
    pub split_segments: Vec<SplitSegment>,
    pub cues_total: usize,
    pub cues_snapped: usize,
    pub overlap_score_before: f64,
    pub overlap_score_after: f64,
    pub max_cue_delta_ms: u64,
    pub median_cue_delta_ms: f64,
}

pub fn sync_subtitles_with_vad(
    doc: &mut SubtitleDocument,
    vad: &VadResult,
    config: &SyncConfig,
) -> Result<SyncReport, SyncError> {
    check_cancelled()?;
    config.validate()?;
    validate_vad(vad)?;
    let alignment = align::align(&doc.cues, doc.format, vad, config)?;
    check_cancelled()?;
    let mut deltas: Vec<_> = doc
        .cues
        .iter()
        .zip(&alignment.cues)
        .map(|(before, after)| before.start_ms.abs_diff(after.start_ms))
        .collect();
    deltas.sort_unstable();
    let mid = deltas.len() / 2;
    let median = if deltas.len().is_multiple_of(2) {
        (deltas[mid - 1] as f64 + deltas[mid] as f64) / 2.0
    } else {
        deltas[mid] as f64
    };
    doc.cues = alignment.cues;
    Ok(SyncReport {
        video_path: None,
        wav_path: None,
        subtitle_in: None,
        subtitle_out: None,
        audio_duration_ms: vad.duration_ms,
        speech_segments_count: vad.segments.len(),
        global_offset_ms: alignment.global_offset_ms,
        fps_ratio: alignment.fps_ratio,
        split_segments: alignment.split_segments,
        cues_total: doc.cues.len(),
        cues_snapped: alignment.cues_snapped,
        overlap_score_before: alignment.overlap_score_before,
        overlap_score_after: alignment.overlap_score_after,
        max_cue_delta_ms: deltas.last().copied().unwrap_or(0),
        median_cue_delta_ms: median,
    })
}

fn validate_vad(vad: &VadResult) -> Result<(), SyncError> {
    if vad.frame_ms != FRAME_MS
        || vad.probability.len() != vad.activity.len()
        || vad.duration_ms.div_ceil(u64::from(FRAME_MS)) != vad.activity.len() as u64
        || vad.probability.iter().any(|p| !(0.0..=1.0).contains(p))
        || vad.activity.iter().any(|&p| p > 1)
        || vad
            .segments
            .iter()
            .any(|s| s.start_ms >= s.end_ms || s.end_ms > vad.duration_ms)
        || vad.segments.windows(2).any(|s| s[0].end_ms > s[1].start_ms)
    {
        return Err(SyncError::Audio(
            "expected a valid 10 ms speech activity timeline".into(),
        ));
    }
    if !vad.activity.contains(&1) || vad.segments.is_empty() {
        return Err(SyncError::NoSpeech);
    }
    if !vad.activity.contains(&0) {
        return Err(SyncError::Audio(
            "speech activity is continuous; no silent boundaries can anchor an alignment".into(),
        ));
    }
    let speech_ms: u64 = vad.segments.iter().map(|s| s.end_ms - s.start_ms).sum();
    // Require at least one second or 1% of the timeline, whichever is shorter.
    if speech_ms < vad.duration_ms.div_ceil(100).min(1000) {
        return Err(SyncError::Audio(
            "too little speech activity to anchor an alignment".into(),
        ));
    }
    Ok(())
}

pub fn sync_file_to_video(
    video: &Path,
    input: &Path,
    output: &Path,
    ffmpeg_bin: &str,
    config: &SyncConfig,
) -> Result<SyncReport, SyncError> {
    sync_file_to_video_with_report(video, input, output, ffmpeg_bin, config, None)
}

#[doc(hidden)]
pub fn sync_file_to_video_with_report(
    video: &Path,
    input: &Path,
    output: &Path,
    ffmpeg_bin: &str,
    config: &SyncConfig,
    json_report: Option<&Path>,
) -> Result<SyncReport, SyncError> {
    sync_file_to_video_with_timeout(
        video,
        input,
        output,
        ffmpeg_bin,
        config,
        json_report,
        audio::DEFAULT_FFMPEG_TIMEOUT,
    )
}

/// Synchronize from video while limiting the FFmpeg extraction time.
pub fn sync_file_to_video_with_timeout(
    video: &Path,
    input: &Path,
    output: &Path,
    ffmpeg_bin: &str,
    config: &SyncConfig,
    json_report: Option<&Path>,
    timeout: Duration,
) -> Result<SyncReport, SyncError> {
    let (bytes, mut report) = prepare_file(input, output, config, || {
        Ok((
            audio::VIDEO_SAMPLE_RATE,
            audio::extract_pcm_via_ffmpeg_with_timeout(video, ffmpeg_bin, timeout)?,
        ))
    })?;
    report.video_path = Some(video.to_string_lossy().into_owned());
    publish_result(output, &bytes, &report, json_report)?;
    Ok(report)
}

pub fn sync_file_to_wav(
    wav: &Path,
    input: &Path,
    output: &Path,
    config: &SyncConfig,
) -> Result<SyncReport, SyncError> {
    sync_file_to_wav_with_report(wav, input, output, config, None)
}

#[doc(hidden)]
pub fn sync_file_to_wav_with_report(
    wav: &Path,
    input: &Path,
    output: &Path,
    config: &SyncConfig,
    json_report: Option<&Path>,
) -> Result<SyncReport, SyncError> {
    let (bytes, mut report) = prepare_file(input, output, config, || audio::read_wav_pcm16(wav))?;
    report.wav_path = Some(wav.to_string_lossy().into_owned());
    publish_result(output, &bytes, &report, json_report)?;
    Ok(report)
}

fn prepare_file(
    input: &Path,
    output: &Path,
    config: &SyncConfig,
    load_audio: impl FnOnce() -> Result<(u32, Vec<i16>), SyncError>,
) -> Result<(Vec<u8>, SyncReport), SyncError> {
    check_cancelled()?;
    config.validate()?;
    if output
        .try_exists()
        .map_err(|e| SyncError::io("Check subtitle output", e))?
    {
        return Err(SyncError::Subtitle(
            "choose a new subtitle output path; existing files are not replaced".into(),
        ));
    }
    let text = String::from_utf8(input::read_bytes(
        input,
        MAX_SUBTITLE_BYTES,
        "Read UTF-8 subtitle input",
    )?)
    .map_err(|error| {
        SyncError::io(
            "Read UTF-8 subtitle input",
            std::io::Error::new(std::io::ErrorKind::InvalidData, error),
        )
    })?;
    let mut doc = SubtitleDocument::parse(&text, Some(input))?;
    if SubtitleFormat::from_path(output).is_some_and(|format| format != doc.format) {
        return Err(SyncError::Subtitle(
            "output extension does not match the input subtitle format".into(),
        ));
    }
    let (rate, pcm) = load_audio()?;
    let vad = extract_speech_activity_cancellable(&pcm, rate, FRAME_MS)?;
    let mut report = sync_subtitles_with_vad(&mut doc, &vad, config)?;
    let bytes = doc.serialize()?.into_bytes();
    report.subtitle_in = Some(input.to_string_lossy().into_owned());
    report.subtitle_out = Some(output.to_string_lossy().into_owned());
    Ok((bytes, report))
}

#[doc(hidden)]
pub fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), SyncError> {
    publication::stage_bytes(path, bytes)?.publish()
}

fn publish_result(
    output: &Path,
    bytes: &[u8],
    report: &SyncReport,
    json_report: Option<&Path>,
) -> Result<(), SyncError> {
    let subtitle = publication::stage_bytes(output, bytes)?;
    let receipt = json_report
        .map(|path| {
            let mut bytes = serde_json::to_vec_pretty(report).map_err(SyncError::Json)?;
            bytes.push(b'\n');
            publication::stage_bytes(path, &bytes)
        })
        .transpose()?;
    // Both payloads are complete before either final name becomes visible.
    publication::publish_pair(subtitle, receipt)
}
