use crate::SyncError;
use rustfft::{FftPlanner, num_complex::Complex};
use std::f32::consts::TAU;

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
    pub segments: Vec<TimeSpan>,
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
        segments: Vec::new(),
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
        features.push((db, zcr_hz, band / total.max(f32::EPSILON)));
    }
    checkpoint()?;
    let mut levels: Vec<_> = features.iter().map(|&(db, _, _)| db).collect();
    let percentile = levels.len() / 5;
    let (_, floor, _) = levels.select_nth_unstable_by(percentile, f32::total_cmp);
    let threshold = (*floor + 8.0).clamp(-55.0, -28.0);
    result.probability = features
        .into_iter()
        .map(|(db, zcr, band)| {
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
    checkpoint()?;
    Ok(result)
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
