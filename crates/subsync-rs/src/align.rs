use crate::{
    SplitSegment, SyncConfig, SyncError, SyncReport, check_cancelled,
    formats::{SubtitleCue, SubtitleFormat},
    vad::VadResult,
};
use rustfft::{FftPlanner, num_complex::Complex};

/// Playback speed candidates supported by the synchronization report protocol.
pub const FPS_RATIOS: [f64; 5] = [1.0, 23.976 / 25.0, 25.0 / 23.976, 24.0 / 25.0, 25.0 / 24.0];
/// Largest supported timeline before additional offset padding is considered.
pub const MAX_TIMELINE_FRAMES: usize = 1 << 22;

pub fn build_subtitle_activity(
    cues: &[SubtitleCue],
    total_frames: usize,
    frame_ms: u32,
) -> Vec<f32> {
    build_activity_with_check(cues, total_frames, frame_ms, || Ok(()))
        .expect("the public rasterizer has an infallible checkpoint")
}

fn build_activity_with_check(
    cues: &[SubtitleCue],
    total_frames: usize,
    frame_ms: u32,
    mut checkpoint: impl FnMut() -> Result<(), SyncError>,
) -> Result<Vec<f32>, SyncError> {
    checkpoint()?;
    let mut activity = vec![0.0_f32; total_frames];
    let frame = u64::from(frame_ms);
    if frame == 0 {
        return Ok(activity);
    }
    for cue in cues.iter().filter(|cue| cue.start_ms < cue.end_ms) {
        checkpoint()?;
        let range = cue.start_ms / frame..cue.end_ms.div_ceil(frame).min(total_frames as u64);
        for index in range {
            if index.is_multiple_of(4096) {
                checkpoint()?;
            }
            let start = index * frame;
            let covered = cue.end_ms.min(start.saturating_add(frame)) - cue.start_ms.max(start);
            let value = &mut activity[index as usize];
            *value = value.max(covered as f32 / frame as f32);
        }
    }
    Ok(activity)
}

struct Global {
    ratio: f64,
    shift: i64,
    candidates: Vec<i64>,
}

fn global_match(
    cues: &[SubtitleCue],
    weights: &[f32],
    frame_ms: u32,
    config: &SyncConfig,
) -> Result<Global, SyncError> {
    check_cancelled()?;
    let frame = i64::from(frame_ms);
    let bound = config.max_offset_ms / frame;
    let margin = config.max_split_offset_ms / frame;
    let lag = (bound + margin) as usize;
    let subtitle_end = cues.iter().map(|cue| cue.end_ms).max().unwrap_or(0);
    let needed = (scale(subtitle_end, FPS_RATIOS[2]).div_ceil(frame as u64))
        .min(weights.len() as u64 + lag as u64) as usize;
    let n = needed
        .max(weights.len())
        .checked_add(lag)
        .and_then(usize::checked_next_power_of_two)
        .filter(|&len| len <= MAX_TIMELINE_FRAMES)
        .ok_or_else(|| SyncError::Audio("timeline exceeds the FFT frame limit".into()))?;
    let mut planner = FftPlanner::<f32>::new();
    let forward = planner.plan_fft_forward(n);
    let inverse = planner.plan_fft_inverse(n);
    let mut scratch = vec![
        Complex::default();
        forward
            .get_inplace_scratch_len()
            .max(inverse.get_inplace_scratch_len())
    ];
    let (audio_sum, audio_energy) = stats(weights, n);
    let mut audio = vec![Complex::default(); n];
    for (bin, &weight) in audio.iter_mut().zip(weights) {
        bin.re = weight;
    }
    forward.process_with_scratch(&mut audio, &mut scratch);
    check_cancelled()?;
    let mut work = vec![Complex::default(); n];
    let mut best_score = f64::NEG_INFINITY;
    let mut solution = Global {
        ratio: 1.0,
        shift: 0,
        candidates: Vec::new(),
    };
    for &ratio in &FPS_RATIOS[..if config.fps_search {
        FPS_RATIOS.len()
    } else {
        1
    }] {
        check_cancelled()?;
        let scaled: Vec<_> = cues.iter().map(|cue| scaled_cue(*cue, ratio)).collect();
        let subtitle = build_activity_with_check(&scaled, needed, frame_ms, check_cancelled)?;
        let (sum, energy) = stats(&subtitle, n);
        let norm = (audio_energy * energy).sqrt();
        if norm <= f64::EPSILON {
            continue;
        }
        work.fill(Complex::default());
        for (bin, weight) in work.iter_mut().zip(subtitle) {
            bin.re = weight;
        }
        forward.process_with_scratch(&mut work, &mut scratch);
        check_cancelled()?;
        for (bin, audio_bin) in work.iter_mut().zip(&audio) {
            *bin = audio_bin * bin.conj();
        }
        inverse.process_with_scratch(&mut work, &mut scratch);
        check_cancelled()?;
        // The shared zero padding permits centered, normalized correlation without wrapping across a searched lag.
        let score = |shift: i64| {
            let dot = f64::from(work[shift.rem_euclid(n as i64) as usize].re) / n as f64;
            (dot - audio_sum * sum / n as f64) / norm
        };
        let mut shift = 0_i64;
        let mut peak = score(0);
        for candidate in -bound..=bound {
            let value = score(candidate);
            if value > peak + 1e-7 || (value - peak).abs() <= 1e-7 && candidate.abs() < shift.abs()
            {
                peak = value;
                shift = candidate;
            }
        }
        if peak <= best_score + 1e-7 {
            continue;
        }
        best_score = peak;
        let step = 100_u32.div_ceil(frame_ms) as i64;
        let mut candidates = vec![shift];
        candidates.extend((-margin / step..=margin / step).map(|i| shift + i * step));
        candidates.extend([shift - margin, shift + margin]);
        let mut peaks: Vec<_> = (shift - margin..=shift + margin)
            .map(|candidate| (score(candidate), candidate))
            .collect();
        peaks.sort_unstable_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then(a.1.abs_diff(shift).cmp(&b.1.abs_diff(shift)))
        });
        let mut selected = vec![shift];
        for (_, candidate) in peaks {
            if selected
                .iter()
                .all(|&other| candidate.abs_diff(other) >= step as u64)
            {
                selected.push(candidate);
                if selected.len() == 13 {
                    break;
                }
            }
        }
        candidates.extend(selected);
        candidates.sort_unstable();
        candidates.dedup();
        solution = Global {
            ratio,
            shift: shift * frame,
            candidates: candidates.into_iter().map(|value| value * frame).collect(),
        };
    }
    if solution.candidates.is_empty() {
        return Err(SyncError::Subtitle(
            "no timed cue is within the audio alignment window".into(),
        ));
    }
    Ok(solution)
}

fn stats(signal: &[f32], n: usize) -> (f64, f64) {
    let (sum, squares) = signal
        .iter()
        .map(|&value| f64::from(value))
        .fold((0.0, 0.0), |(s, sq), value| (s + value, sq + value * value));
    (sum, (squares - sum * sum / n as f64).max(0.0))
}

fn scale(ms: u64, ratio: f64) -> u64 {
    (ms as f64 * ratio).round() as u64
}

fn scaled_cue(cue: SubtitleCue, ratio: f64) -> SubtitleCue {
    let start_ms = scale(cue.start_ms, ratio);
    SubtitleCue {
        start_ms,
        end_ms: scale(cue.end_ms, ratio).max(start_ms.saturating_add(1)),
    }
}

struct Integral<'a, T> {
    frames: &'a [T],
    prefix: Vec<f64>,
    frame_ms: u64,
    duration_ms: u64,
}

impl<'a, T: Copy + Into<f64>> Integral<'a, T> {
    fn new(frames: &'a [T], vad: &VadResult) -> Self {
        let mut prefix = Vec::with_capacity(frames.len() + 1);
        prefix.push(0.0);
        let mut sum = 0.0;
        for &value in frames {
            sum += value.into();
            prefix.push(sum);
        }
        Self {
            frames,
            prefix,
            frame_ms: u64::from(vad.frame_ms),
            duration_ms: vad.duration_ms,
        }
    }

    fn at(&self, ms: u64) -> f64 {
        let ms = ms.min(self.duration_ms);
        let index = (ms / self.frame_ms).min(self.frames.len() as u64) as usize;
        self.prefix[index] * self.frame_ms as f64
            + self.frames.get(index).map_or(0.0, |&value| value.into())
                * (ms % self.frame_ms) as f64
    }

    fn fraction(&self, start: u64, end: u64) -> f64 {
        (self.at(end) - self.at(start)) / (end - start) as f64
    }
}

#[derive(Clone, Copy)]
struct Group {
    first: usize,
    end: usize,
    start_ms: u64,
    end_ms: u64,
}

fn groups(cues: &[SubtitleCue], order: &[usize], format: SubtitleFormat) -> Vec<Group> {
    let mut groups = Vec::<Group>::new();
    for (i, &index) in order.iter().enumerate() {
        let cue = cues[index];
        if format == SubtitleFormat::Ass
            && let Some(group) = groups.last_mut()
            && cue.start_ms < group.end_ms
        {
            group.end = i + 1;
            group.end_ms = group.end_ms.max(cue.end_ms);
        } else {
            groups.push(Group {
                first: i,
                end: i + 1,
                start_ms: cue.start_ms,
                end_ms: cue.end_ms,
            });
        }
    }
    groups
}

struct TimingPlan {
    base: Vec<SubtitleCue>,
    order: Vec<usize>,
    groups: Vec<Group>,
    format: SubtitleFormat,
    unit: u64,
}

impl TimingPlan {
    fn new(cues: &[SubtitleCue], format: SubtitleFormat, ratio: f64) -> Self {
        let unit = format.precision();
        let base: Vec<_> = cues
            .iter()
            .map(|cue| {
                let scaled = scaled_cue(*cue, ratio);
                let start_ms = scaled.start_ms / unit * unit;
                SubtitleCue {
                    start_ms,
                    end_ms: (scaled.end_ms / unit * unit).max(start_ms + unit),
                }
            })
            .collect();
        let mut order: Vec<_> = (0..cues.len()).collect();
        order.sort_unstable_by_key(|&i| (cues[i].start_ms, i));
        let groups = groups(&base, &order, format);
        Self {
            base,
            order,
            groups,
            format,
            unit,
        }
    }

    fn shift_group(
        &self,
        group: &Group,
        shift: i64,
        previous_end: u64,
        aligned: &mut [SubtitleCue],
    ) -> (i64, u64) {
        let shift = shift.div_euclid(self.unit as i64) * self.unit as i64;
        let actual = if self.format == SubtitleFormat::Ass {
            shift.max(previous_end as i64 - group.start_ms as i64)
        } else {
            shift.max(-(group.start_ms as i64))
        };
        for &i in &self.order[group.first..group.end] {
            aligned[i].start_ms = self.base[i].start_ms.saturating_add_signed(actual);
            aligned[i].end_ms = self.base[i].end_ms.saturating_add_signed(actual);
        }
        (shift, group.end_ms.saturating_add_signed(actual))
    }

    fn repair(&self, aligned: &mut [SubtitleCue]) {
        if self.format == SubtitleFormat::Srt {
            for i in 1..aligned.len() {
                let current = aligned[i].start_ms.max(aligned[i - 1].start_ms + self.unit);
                aligned[i - 1].end_ms = aligned[i - 1].end_ms.min(current);
                aligned[i].start_ms = current;
                aligned[i].end_ms = aligned[i].end_ms.max(current + self.unit);
            }
        }
    }
}

/// Verify both returned endpoints against the engine's reported transform and permitted snapping.
pub fn validate_reported_timing(
    cues: &[SubtitleCue],
    output: &[SubtitleCue],
    format: SubtitleFormat,
    report: &SyncReport,
    config: &SyncConfig,
) -> Result<(), SyncError> {
    config.validate()?;
    validate_cues(cues, format)?;
    let invalid = || {
        SyncError::Subtitle(
            "worker timestamps do not match the permitted reported transformation".into(),
        )
    };
    if cues.len() != output.len()
        || report.cues_total != cues.len()
        || !FPS_RATIOS
            .iter()
            .take(if config.fps_search {
                FPS_RATIOS.len()
            } else {
                1
            })
            .any(|ratio| (ratio - report.fps_ratio).abs() <= 1e-9)
        || report.global_offset_ms.unsigned_abs() > config.max_offset_ms as u64
        || report.split_segments.iter().any(|segment| {
            segment.shift_ms.abs_diff(report.global_offset_ms) > config.max_split_offset_ms as u64
                || segment.shift_ms % format.precision() as i64 != 0
        })
    {
        return Err(invalid());
    }
    let mut shifts = Vec::with_capacity(cues.len());
    for segment in &report.split_segments {
        if shifts.len().checked_add(1) != Some(segment.start_cue)
            || segment.end_cue < segment.start_cue
            || segment.end_cue > cues.len()
        {
            return Err(invalid());
        }
        shifts.resize(segment.end_cue, segment.shift_ms);
    }
    if shifts.len() != cues.len() {
        return Err(invalid());
    }
    let plan = TimingPlan::new(cues, format, report.fps_ratio);
    let mut expected = plan.base.clone();
    let mut previous_end = 0;
    for group in &plan.groups {
        let shift = shifts[group.first];
        if shifts[group.first..group.end]
            .iter()
            .any(|&other| other != shift)
        {
            return Err(invalid());
        }
        previous_end = plan
            .shift_group(group, shift, previous_end, &mut expected)
            .1;
    }
    plan.repair(&mut expected);
    let mut snapped = 0;
    let mut previous_end = 0;
    for group in &plan.groups {
        let indices = &plan.order[group.first..group.end];
        for &i in indices {
            let (before, after) = (expected[i], output[i]);
            if after.end_ms != before.end_ms || after.start_ms >= after.end_ms {
                return Err(invalid());
            }
            if after.start_ms != before.start_ms {
                if indices.len() != 1
                    || config.snap_ms == 0
                    || before.end_ms - before.start_ms < 500
                    || after.start_ms.abs_diff(before.start_ms) > config.snap_ms
                    || after.start_ms < previous_end
                    || after.end_ms - after.start_ms < 500
                    || after.start_ms >= report.audio_duration_ms
                    || !after.start_ms.is_multiple_of(plan.unit)
                {
                    return Err(invalid());
                }
                snapped += 1;
            }
        }
        previous_end = indices
            .iter()
            .map(|&i| expected[i].end_ms)
            .max()
            .unwrap_or(previous_end);
    }
    if snapped != report.cues_snapped {
        return Err(invalid());
    }
    Ok(())
}

fn local_shifts(
    cues: &[SubtitleCue],
    order: &[usize],
    groups: &[Group],
    speech: &Integral<'_, f32>,
    global: &Global,
    penalty: f64,
) -> Result<Vec<i64>, SyncError> {
    let offsets = &global.candidates;
    let n = offsets.len();
    let states = groups
        .len()
        .checked_mul(n)
        .filter(|&states| states <= 32_000_000)
        .ok_or_else(|| SyncError::Subtitle("too many cue and offset combinations".into()))?;
    let mut traceback = vec![0_u32; states];
    let mut previous = vec![0.0_f64; n];
    let mut current = vec![0.0_f64; n];
    let better = |i: usize, j: usize, scores: &[f64]| {
        scores[i] > scores[j] + 1e-9
            || ((scores[i] - scores[j]).abs() <= 1e-9
                && offsets[i].abs_diff(global.shift) < offsets[j].abs_diff(global.shift))
    };
    for (step, group) in groups.iter().enumerate() {
        check_cancelled()?;
        // An earlier offset is legal if its previous cue or overlap group ends before this one starts.
        let room = step.checked_sub(1).map_or(i64::MAX, |prev| {
            group.start_ms.saturating_sub(groups[prev].end_ms) as i64
        });
        let mut best = 0;
        let mut limit = 0;
        for (j, &shift) in offsets.iter().enumerate() {
            if j.is_multiple_of(64) {
                check_cancelled()?;
            }
            let legal = shift.saturating_add(room);
            while limit < n && offsets[limit] <= legal {
                if better(limit, best, &previous) {
                    best = limit;
                }
                limit += 1;
            }
            let jump = previous[best] - penalty;
            let (score, parent) = if jump > previous[j] {
                (jump, best)
            } else {
                (previous[j], j)
            };
            let relative_shift = shift.max(-(group.start_ms as i64));
            let value: f64 = order[group.first..group.end]
                .iter()
                .map(|&index| {
                    let cue = cues[index];
                    let start = cue.start_ms.saturating_add_signed(relative_shift);
                    let end = cue.end_ms.saturating_add_signed(relative_shift);
                    2.0 * speech.fraction(start, end) - 1.0
                })
                .sum();
            current[j] = score + value;
            traceback[step * n + j] = parent as u32;
        }
        std::mem::swap(&mut previous, &mut current);
    }
    let mut j = 0;
    for i in 1..n {
        if better(i, j, &previous) {
            j = i;
        }
    }
    let mut result = vec![0; groups.len()];
    for i in (0..groups.len()).rev() {
        result[i] = offsets[j];
        j = traceback[i * n + j] as usize;
    }
    Ok(result)
}

fn score<T: Copy + Into<f64>>(cues: &[SubtitleCue], speech: &Integral<'_, T>) -> f64 {
    let total: f64 = cues
        .iter()
        .map(|cue| (cue.end_ms - cue.start_ms) as f64)
        .sum();
    let matched: f64 = cues
        .iter()
        .map(|cue| speech.at(cue.end_ms) - speech.at(cue.start_ms))
        .sum();
    (matched / total).clamp(0.0, 1.0)
}

fn snap_start(
    cue: &mut SubtitleCue,
    min_start: u64,
    vad: &VadResult,
    binary: &Integral<'_, u8>,
    distance: u64,
    unit: u64,
) -> bool {
    if distance == 0 || cue.end_ms - cue.start_ms < 500 {
        return false;
    }
    let low = cue.start_ms.saturating_sub(distance);
    let high = cue.start_ms.saturating_add(distance);
    let first = vad.segments.partition_point(|span| span.start_ms < low);
    let baseline = binary.fraction(cue.start_ms, cue.end_ms);
    let candidate = vad.segments[first..]
        .iter()
        .take_while(|span| span.start_ms <= high)
        .map(|span| span.start_ms / unit * unit)
        .filter(|&start| start >= low && start >= min_start && start <= cue.end_ms - 500)
        .filter(|&start| binary.fraction(start, cue.end_ms) > baseline + 1e-9)
        .min_by_key(|&start| start.abs_diff(cue.start_ms));
    if let Some(start) = candidate {
        cue.start_ms = start;
        true
    } else {
        false
    }
}

pub(crate) struct Alignment {
    pub cues: Vec<SubtitleCue>,
    pub fps_ratio: f64,
    pub global_offset_ms: i64,
    pub split_segments: Vec<SplitSegment>,
    pub cues_snapped: usize,
    pub overlap_score_before: f64,
    pub overlap_score_after: f64,
}

pub(crate) fn align(
    cues: &[SubtitleCue],
    format: SubtitleFormat,
    vad: &VadResult,
    config: &SyncConfig,
) -> Result<Alignment, SyncError> {
    check_cancelled()?;
    validate_cues(cues, format)?;
    let weights: Vec<_> = vad
        .probability
        .iter()
        .zip(&vad.activity)
        .map(|(&p, &active)| if active == 0 { 0.0 } else { p.max(0.16) })
        .collect();
    let global = global_match(cues, &weights, vad.frame_ms, config)?;
    let plan = TimingPlan::new(cues, format, global.ratio);
    let speech = Integral::new(&weights, vad);
    let binary = Integral::new(&vad.activity, vad);
    let offsets = local_shifts(
        &plan.base,
        &plan.order,
        &plan.groups,
        &speech,
        &global,
        config.split_penalty,
    )?;
    let mut aligned = plan.base.clone();
    let mut split_segments: Vec<SplitSegment> = Vec::new();
    let mut previous_end = 0_u64;
    for (group, shift) in plan.groups.iter().zip(offsets) {
        check_cancelled()?;
        let (shift, end) = plan.shift_group(group, shift, previous_end, &mut aligned);
        if let Some(previous) = split_segments
            .last_mut()
            .filter(|segment| segment.shift_ms == shift)
        {
            previous.end_cue = group.end;
        } else {
            split_segments.push(SplitSegment {
                start_cue: group.first + 1,
                end_cue: group.end,
                shift_ms: shift,
            });
        }
        previous_end = end;
    }
    plan.repair(&mut aligned);
    let mut cues_snapped = 0;
    let mut previous_end = 0;
    for group in &plan.groups {
        check_cancelled()?;
        let indices = &plan.order[group.first..group.end];
        if let [i] = indices {
            cues_snapped += usize::from(snap_start(
                &mut aligned[*i],
                previous_end,
                vad,
                &binary,
                config.snap_ms,
                plan.unit,
            ));
        }
        previous_end = indices
            .iter()
            .map(|&i| aligned[i].end_ms)
            .max()
            .unwrap_or(previous_end);
    }
    Ok(Alignment {
        overlap_score_before: score(cues, &binary),
        overlap_score_after: score(&aligned, &binary),
        cues: aligned,
        fps_ratio: global.ratio,
        global_offset_ms: global.shift,
        split_segments,
        cues_snapped,
    })
}

fn validate_cues(cues: &[SubtitleCue], format: SubtitleFormat) -> Result<(), SyncError> {
    if cues.is_empty()
        || cues
            .iter()
            .any(|cue| cue.start_ms >= cue.end_ms || cue.end_ms > 86_400_000)
    {
        return Err(SyncError::Subtitle(
            "expected positive cue durations and timestamps within 24 hours".into(),
        ));
    }
    if format == SubtitleFormat::Srt
        && cues
            .windows(2)
            .any(|pair| pair[0].start_ms > pair[1].start_ms)
    {
        return Err(SyncError::Subtitle(
            "SRT cues must be in start-time order".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;

    #[test]
    fn a_long_subtitle_cue_can_cancel_during_rasterization() {
        let mut calls = 0;
        let result = build_activity_with_check(
            &[SubtitleCue {
                start_ms: 0,
                end_ms: 100000,
            }],
            10000,
            10,
            || {
                calls += 1;
                if calls == 4 {
                    Err(SyncError::Cancelled)
                } else {
                    Ok(())
                }
            },
        );
        assert!(matches!(result, Err(SyncError::Cancelled)));
        assert_eq!(calls, 4);
    }
}
