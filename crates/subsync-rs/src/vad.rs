use crate::SyncError;
use rustfft::{FftPlanner, num_complex::Complex};
use std::f32::consts::TAU;

pub(crate) const MAX_ONSET_LOOKBACK_MS: u64 = 380;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimeSpan {
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug)]
pub struct VadResult {
    pub frame_ms: u32,
    pub duration_ms: u64,
    pub probability: Vec<f32>,
    pub activity: Vec<u8>,
    /// Scene-adaptive voiced speech frames (`120-3400 Hz`, local floor + 8 dB).
    /// Empty means fall back to `activity` (for synthetic test fixtures).
    pub voiced_activity: Vec<u8>,
    pub segments: Vec<TimeSpan>,
    /// Primary activity before gap bridging and tail padding.
    pub raw_segments: Vec<TimeSpan>,
    /// Onsets for snapping only, in the same order as `segments`.
    pub onset_anchors_ms: Vec<u64>,
}

pub fn extract_speech_activity(samples: &[i16], sample_rate: u32, frame_ms: u32) -> VadResult {
    extract_with_check(samples, sample_rate, frame_ms, || Ok(()))
        .expect("the public detector has an infallible checkpoint")
}

pub(crate) fn extract_speech_activity_cancellable(
    samples: &[i16],
    sample_rate: u32,
    frame_ms: u32,
) -> Result<VadResult, SyncError> {
    extract_with_check(samples, sample_rate, frame_ms, crate::check_cancelled)
}

fn extract_with_check(
    samples: &[i16],
    sample_rate: u32,
    frame_ms: u32,
    mut checkpoint: impl FnMut() -> Result<(), SyncError>,
) -> Result<VadResult, SyncError> {
    checkpoint()?;
    let mut result = VadResult {
        frame_ms,
        duration_ms: 0,
        probability: Vec::new(),
        activity: Vec::new(),
        voiced_activity: Vec::new(),
        segments: Vec::new(),
        raw_segments: Vec::new(),
        onset_anchors_ms: Vec::new(),
    };
    if samples.is_empty()
        || !(8000..=192_000).contains(&sample_rate)
        || !(1..=100).contains(&frame_ms)
    {
        return Ok(result);
    }
    let window_len = sample_rate as usize / 50;
    let fft_len = window_len.next_power_of_two().max(512);
    let fft = FftPlanner::<f32>::new().plan_fft_forward(fft_len);
    let mut scratch = vec![Complex::default(); fft.get_inplace_scratch_len()];
    let mut spectrum = vec![Complex::default(); fft_len];
    let first_bin = (300 * fft_len).div_ceil(sample_rate as usize);
    let low_bin = (120 * fft_len).div_ceil(sample_rate as usize);
    let last_bin = (3400 * fft_len / sample_rate as usize).min(fft_len / 2);
    let window: Vec<_> = (0..window_len)
        .map(|i| 0.5 - 0.5 * (TAU * i as f32 / (window_len - 1) as f32).cos())
        .collect();
    let hop_numerator = u64::from(sample_rate) * u64::from(frame_ms);
    result.duration_ms = (samples.len() as u64)
        .saturating_mul(1000)
        .div_ceil(u64::from(sample_rate));
    let frames = result.duration_ms.div_ceil(u64::from(frame_ms)) as usize;
    let mut features = Vec::with_capacity(frames);
    for i in 0..frames {
        if i.is_multiple_of(64) {
            checkpoint()?;
        }
        let center = ((i as u64 * hop_numerator + hop_numerator / 2) / 1000) as usize;
        let mut energy = 0.0;
        let mut crossings = 0;
        let mut previous = 0;
        for (i, &weight) in window.iter().enumerate() {
            let sample = (center + i)
                .checked_sub(window_len / 2)
                .and_then(|index| samples.get(index))
                .copied()
                .unwrap_or(0);
            crossings += usize::from(i > 0 && (sample < 0) != (previous < 0));
            previous = sample;
            let value = f32::from(sample) / 32_768.0;
            energy += value * value;
            spectrum[i] = Complex::new(value * weight, 0.0);
        }
        spectrum[window_len..].fill(Complex::default());
        fft.process_with_scratch(&mut spectrum, &mut scratch);
        let total = spectrum[1..=fft_len / 2]
            .iter()
            .map(Complex::norm_sqr)
            .sum::<f32>();
        let band = spectrum[first_bin..=last_bin]
            .iter()
            .map(Complex::norm_sqr)
            .sum::<f32>();
        let db = 10.0 * (energy / window_len as f32).max(1e-10).log10();
        let zcr_hz = crossings as f32 * sample_rate as f32 / (window_len - 1) as f32;
        let denominator = total.max(f32::EPSILON);
        let low_band = spectrum[low_bin..first_bin]
            .iter()
            .map(Complex::norm_sqr)
            .sum::<f32>();
        features.push((
            db,
            zcr_hz,
            band / denominator,
            (band + low_band) / denominator,
        ));
    }
    checkpoint()?;
    let mut levels: Vec<_> = features.iter().map(|&(db, _, _, _)| db).collect();
    let percentile = levels.len() / 5;
    let (_, floor, _) = levels.select_nth_unstable_by(percentile, f32::total_cmp);
    let threshold = (*floor + 8.0).clamp(-55.0, -28.0);
    result.probability = features
        .iter()
        .map(|&(db, zcr, band, _)| {
            let loudness = ((db - threshold + 3.0) / 14.0).clamp(0.0, 1.0);
            let spectral = ((band - 0.28) / 0.49).clamp(0.0, 1.0);
            let voiced =
                ((zcr - 35.0) / 200.0).clamp(0.0, 1.0) * ((7400.0 - zcr) / 3400.0).clamp(0.0, 1.0);
            loudness * (0.76 * spectral + 0.24 * voiced)
        })
        .collect();
    let mut segments = Vec::<(usize, usize)>::new();
    let mut cursor = 0;
    while cursor < frames {
        if cursor.is_multiple_of(2048) {
            checkpoint()?;
        }
        if result.probability[cursor] < 0.52 {
            cursor += 1;
            continue;
        }
        let start = cursor;
        while cursor < frames && result.probability[cursor] >= 0.52 {
            if cursor.is_multiple_of(2048) {
                checkpoint()?;
            }
            cursor += 1;
        }
        result.raw_segments.push(TimeSpan {
            start_ms: start as u64 * u64::from(frame_ms),
            end_ms: (cursor as u64 * u64::from(frame_ms)).min(result.duration_ms),
        });
        if let Some((_, end)) = segments.last_mut()
            && start - *end <= (200 / frame_ms) as usize
        {
            *end = cursor;
        } else {
            segments.push((start, cursor));
        }
    }
    result.activity = vec![0; frames];
    let frame = u64::from(frame_ms);
    for (start, end) in segments {
        if end - start < 60_u32.div_ceil(frame_ms) as usize {
            continue;
        }
        let end = (end + (100 / frame_ms) as usize).min(frames);
        result.activity[start..end].fill(1);
    }
    let mut i = 0;
    while i < frames {
        if i.is_multiple_of(2048) {
            checkpoint()?;
        }
        if result.activity[i] == 0 {
            i += 1;
            continue;
        }
        let start = i;
        while i < frames && result.activity[i] != 0 {
            if i.is_multiple_of(2048) {
                checkpoint()?;
            }
            i += 1;
        }
        result.segments.push(TimeSpan {
            start_ms: start as u64 * frame,
            end_ms: (i as u64 * frame).min(result.duration_ms),
        });
    }
    let (voiced_activity, onset_anchors_ms) =
        voiced_signals(&features, &result.segments, frame_ms, checkpoint)?;
    result.voiced_activity = voiced_activity;
    result.onset_anchors_ms = onset_anchors_ms;
    Ok(result)
}

fn voiced_signals(
    features: &[(f32, f32, f32, f32)],
    segments: &[TimeSpan],
    frame_ms: u32,
    mut checkpoint: impl FnMut() -> Result<(), SyncError>,
) -> Result<(Vec<u8>, Vec<u64>), SyncError> {
    if features.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let step = (1000 / frame_ms) as usize;
    let radius = (12_500 / frame_ms) as usize;
    let mut floors = Vec::with_capacity(features.len().div_ceil(step));
    let mut levels = Vec::with_capacity(radius * 2);
    for i in (0..features.len()).step_by(step) {
        checkpoint()?;
        levels.clear();
        levels.extend(
            features[i.saturating_sub(radius)..(i + radius).min(features.len())]
                .iter()
                .map(|f| f.0),
        );
        let percentile = levels.len() / 5;
        let (_, floor, _) = levels.select_nth_unstable_by(percentile, f32::total_cmp);
        floors.push(floor.clamp(-56.0, -30.0));
    }
    let local_floor = |i: usize| {
        let left = floors[i / step];
        let right = floors.get(i / step + 1).copied().unwrap_or(left);
        left + (right - left) * ((i % step) as f32 / step as f32)
    };
    let qualifies = |i: usize| {
        let (db, zcr, upper, broad) = features[i];
        let floor = local_floor(i);
        broad >= 0.65 && upper >= 0.06 && (350.0..=3800.0).contains(&zcr) && db >= floor + 8.0
    };
    let voiced_activity: Vec<u8> = (0..features.len())
        .map(|i| u8::from(qualifies(i)))
        .collect();
    if segments.is_empty() {
        checkpoint()?;
        return Ok((voiced_activity, Vec::new()));
    }
    let qualifies_fricative = |i: usize| {
        let (db, zcr, _, _) = features[i];
        zcr >= 3800.0 && db >= (local_floor(i) + 8.0).max(-40.0)
    };
    let frame = u64::from(frame_ms);
    let persistence = 40_u32.div_ceil(frame_ms) as usize;
    let lookback = (MAX_ONSET_LOOKBACK_MS / frame) as usize;
    let closure = (180 / frame_ms) as usize;
    let crossing = 30_u32.div_ceil(frame_ms) as usize;
    let contrast = 150_u32.div_ceil(frame_ms) as usize;
    let fricative_gap = (60 / frame_ms) as usize;
    let mean_db = |start: usize, end: usize| {
        features[start..end].iter().map(|f| f.0).sum::<f32>() / (end - start) as f32
    };
    let mean_floor = |start: usize, end: usize| {
        (start..end).map(&local_floor).sum::<f32>() / (end - start) as f32
    };
    let mut previous_end = 0;
    let mut anchors = Vec::with_capacity(segments.len());
    for segment in segments {
        checkpoint()?;
        let start = (segment.start_ms / frame) as usize;
        let lower = start.saturating_sub(lookback).max(previous_end);
        let upper = (start + crossing).min(features.len());
        previous_end = segment.end_ms.div_ceil(frame) as usize;
        let confirmed_end = (start + (100 / frame_ms) as usize).min(features.len());
        let confirmed_db = if start < confirmed_end {
            mean_db(start, confirmed_end)
        } else {
            -60.0
        };
        let mut candidate = None;
        let mut i = lower;
        while i < upper {
            if !qualifies(i) {
                i += 1;
                continue;
            }
            let run_start = i;
            while i < upper && qualifies(i) {
                i += 1;
            }
            // A run already active before the clipped window has no onset at its lower bound.
            if run_start == lower && lower > 0 && qualifies(lower - 1) {
                continue;
            }
            if candidate.is_some() && run_start + crossing <= start && i >= start + crossing {
                candidate = None;
                break;
            }
            let run_end = i.min(start);
            if run_end.saturating_sub(run_start) < persistence || start - run_end > closure {
                continue;
            }
            let run_db = mean_db(run_start, run_end);
            // Ignore a faint separated pre-speech ambient bump near the local floor.
            if start - run_end >= crossing
                && run_db + 12.0 < confirmed_db
                && run_db < mean_floor(run_start, run_end) + 12.0
            {
                continue;
            }
            let before = run_start.saturating_sub(contrast);
            if before < run_start {
                let background = mean_db(before, run_start);
                if background > -30.0 && run_db - background < 4.0 {
                    continue;
                }
            }
            candidate.get_or_insert(run_start);
        }
        let mut anchor = candidate.unwrap_or(start);
        // Backtrack a crisp unvoiced fricative/sibilant attack (e.g. /s/) emerging from quiet silence.
        let mut j = lower;
        while j < anchor {
            if !qualifies_fricative(j) {
                j += 1;
                continue;
            }
            let run_start = j;
            while j < anchor && qualifies_fricative(j) {
                j += 1;
            }
            if run_start == lower && lower > 0 && qualifies_fricative(lower - 1) {
                continue;
            }
            if j - run_start < persistence || anchor - j > fricative_gap {
                continue;
            }
            let before = run_start.saturating_sub(contrast);
            if before < run_start {
                let background = mean_db(before, run_start);
                let floor_bg = mean_floor(before, run_start);
                if background <= -40.0
                    && background <= floor_bg + 5.0
                    && mean_db(run_start, j) >= background + 6.0
                {
                    anchor = run_start;
                    break;
                }
            }
        }
        anchors.push(anchor as u64 * frame);
    }
    checkpoint()?;
    Ok((voiced_activity, anchors))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_short_pauses_and_rejects_isolated_bursts() {
        let samples: Vec<_> = (0..24_000)
            .map(|i| {
                let ms = i / 16;
                if (100..350).contains(&ms)
                    || (500..800).contains(&ms)
                    || (1250..1270).contains(&ms)
                {
                    (7000.0 * (TAU * i as f32 * 400.0 / 16_000.0).sin()) as i16
                } else {
                    0
                }
            })
            .collect();
        let vad = extract_speech_activity(&samples, 16_000, 10);
        assert_eq!(vad.segments.len(), 1);
        assert!(vad.segments[0].start_ms.abs_diff(100) <= 10);
        assert!(vad.segments[0].end_ms.abs_diff(900) <= 10);
        assert!(vad.probability[45] < vad.probability[55]);
        assert_eq!(vad.activity[45], 1);
        assert_eq!(vad.activity[125], 0);
    }
    #[test]
    fn long_detection_can_cancel_between_spectral_frames() {
        let samples = vec![100; 16000 * 4];
        let mut calls = 0;
        let result = extract_with_check(&samples, 16000, 10, || {
            calls += 1;
            if calls == 3 {
                Err(SyncError::Cancelled)
            } else {
                Ok(())
            }
        });
        assert!(matches!(result, Err(SyncError::Cancelled)));
        assert_eq!(calls, 3);
    }
}
