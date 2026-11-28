use super::{Integral, SyncConfig, SyncError, TimingPlan, check_cancelled};
use crate::vad::{MAX_ONSET_LOOKBACK_MS, TimeSpan, VadResult};

pub(super) struct CueIntegrals<'a> {
    pub binary: &'a Integral<'a, u8>,
    pub speech: &'a Integral<'a, f32>,
    pub voiced_only: &'a Integral<'a, u8>,
    pub voiced_union: &'a Integral<'a, u8>,
}

pub(super) fn refine_group_shifts(
    plan: &TimingPlan,
    signals: &CueIntegrals<'_>,
    segments: &[TimeSpan],
    global_shift: i64,
    config: &SyncConfig,
    offsets: &mut [i64],
) -> Result<(), SyncError> {
    let groups = &plan.groups;
    let limit = config.max_split_offset_ms;
    let frame = signals.speech.frame_ms as i64;
    let mut first = 0;
    let mut previous_end = 0;
    while first < groups.len() {
        let mut end = first + 1;
        while end < groups.len()
            && offsets[end] == offsets[first]
            && joined_gap(groups[end - 1].end_ms, groups[end].start_ms).is_some()
        {
            end += 1;
        }
        let base = offsets[first];
        let next_start = groups.get(end).map_or(signals.speech.duration_ms, |next| {
            next.start_ms.saturating_add_signed(offsets[end])
        });
        let indices = &plan.order[groups[first].first..groups[end - 1].end];
        let score = |shift| score_shift(plan, signals, config.lead_in_ms, indices, shift);
        let (baseline, base_bin, _) = score(base);
        let first_cue = plan.base[indices[0]];
        let first_start = first_cue.start_ms.saturating_add_signed(base);
        let first_end = first_cue.end_ms.saturating_add_signed(base);
        let open_end = first_end.min(first_start.saturating_add(350));
        let open_voiced = if first_start < open_end {
            signals.voiced_union.fraction(first_start, open_end)
        } else {
            0.0
        };
        let base_voiced = group_fraction(plan, signals.voiced_union, indices, base);
        let anchored = open_voiced >= 0.60
            || segments.iter().any(|seg| {
                seg.start_ms.saturating_add(80) >= first_start
                    && seg.start_ms <= first_start.saturating_add(150)
                    && seg.end_ms > first_start
            });
        let already_covered = base_bin >= 0.65 || (base_bin >= 0.45 && base_voiced >= 0.75);
        let radius = limit.min(if base_bin >= 0.35 { 800 } else { 1500 });
        let low = (global_shift - limit)
            .max(base - radius)
            .max(previous_end as i64 - groups[first].start_ms as i64);
        let high = (global_shift + limit)
            .min(base + radius)
            .min(next_start as i64 - groups[end - 1].end_ms as i64);
        let mut best = base;
        let mut best_score = baseline;
        if (low..=high).contains(&base) {
            if !anchored && !already_covered {
                for step in -((base - low) / frame)..=(high - base) / frame {
                    check_cancelled()?;
                    let candidate = base + step * frame;
                    if candidate == base {
                        continue;
                    }
                    let (raw, cand_bin, cand_onset) = score(candidate);
                    if candidate.abs_diff(base) > 200
                        && (base_bin >= 0.35 || open_voiced >= 0.40)
                        && cand_onset < 0.25
                    {
                        continue;
                    }
                    if cand_bin < base_bin + 0.15 {
                        continue;
                    }
                    let value = raw - 0.00020 * candidate.abs_diff(base) as f64;
                    if value > baseline + 0.10
                        && (value > best_score + 1e-9
                            || ((value - best_score).abs() <= 1e-9
                                && candidate.abs_diff(base) < best.abs_diff(base)))
                    {
                        best = candidate;
                        best_score = value;
                    }
                }
            }
            offsets[first..end].fill(best);
            for g in first..end {
                let group = &groups[g];
                let prior = if g == first {
                    previous_end
                } else {
                    groups[g - 1].end_ms.saturating_add_signed(offsets[g - 1])
                };
                let next = if g + 1 == end {
                    next_start
                } else {
                    groups[g + 1].start_ms.saturating_add_signed(best)
                };
                let mut lo = (best - 80)
                    .max(global_shift - limit)
                    .max(prior as i64 - group.start_ms as i64);
                let mut hi = (best + 80)
                    .min(global_shift + limit)
                    .min(next as i64 - group.end_ms as i64);
                if g > first {
                    let gap = group.start_ms.saturating_sub(groups[g - 1].end_ms);
                    hi = hi.min(prior as i64 + gap as i64 + 80 - group.start_ms as i64);
                }
                if g + 1 < end {
                    let gap = groups[g + 1].start_ms.saturating_sub(group.end_ms);
                    lo = lo.max(next as i64 - gap as i64 - 80 - group.end_ms as i64);
                }
                if end - first == 1 || !(lo..=hi).contains(&best) {
                    continue;
                }
                let indices = &plan.order[group.first..group.end];
                let (baseline, _, _) = score_shift(plan, signals, config.lead_in_ms, indices, best);
                let mut top = baseline + 0.05;
                for step in -((best - lo) / frame)..=(hi - best) / frame {
                    check_cancelled()?;
                    let candidate = best + step * frame;
                    let (raw, _, _) =
                        score_shift(plan, signals, config.lead_in_ms, indices, candidate);
                    let value = raw - 0.00004 * candidate.abs_diff(best) as f64;
                    if value > top + 1e-9
                        || value > baseline + 0.05
                            && (value - top).abs() <= 1e-9
                            && candidate.abs_diff(best) < offsets[g].abs_diff(best)
                    {
                        top = value;
                        offsets[g] = candidate;
                    }
                }
            }
        }
        for (group, &shift) in groups[first..end].iter().zip(&offsets[first..end]) {
            let minimum = if plan.format == super::SubtitleFormat::Ass {
                previous_end
            } else {
                0
            };
            previous_end = group
                .end_ms
                .saturating_add_signed(shift.max(minimum as i64 - group.start_ms as i64));
        }
        first = end;
    }
    Ok(())
}

pub(super) fn joined_gap(previous_end: u64, next_start: u64) -> Option<u64> {
    let gap = next_start.saturating_sub(previous_end);
    (gap <= 150).then_some(gap)
}

fn group_fraction(
    plan: &TimingPlan,
    integral: &Integral<'_, u8>,
    indices: &[usize],
    shift: i64,
) -> f64 {
    let duration = indices
        .iter()
        .map(|&i| plan.base[i].end_ms - plan.base[i].start_ms)
        .sum::<u64>() as f64;
    if duration <= 0.0 {
        return 0.0;
    }
    let matched: f64 = indices
        .iter()
        .map(|&i| {
            let cue = plan.base[i];
            let start = cue.start_ms.saturating_add_signed(shift);
            let end = cue.end_ms.saturating_add_signed(shift);
            integral.at(end) - integral.at(start)
        })
        .sum();
    matched / duration
}

fn score_shift(
    plan: &TimingPlan,
    signals: &CueIntegrals<'_>,
    lead: u64,
    indices: &[usize],
    shift: i64,
) -> (f64, f64, f64) {
    let frame = signals.speech.frame_ms;
    let duration = indices
        .iter()
        .map(|&i| plan.base[i].end_ms - plan.base[i].start_ms)
        .sum::<u64>() as f64;
    let mut binary_matched = 0.0;
    let mut speech_matched = 0.0;
    let mut onset = 0.0;
    for &i in indices {
        let cue = plan.base[i];
        let start = cue.start_ms.saturating_add_signed(shift);
        let end = cue.end_ms.saturating_add_signed(shift);
        binary_matched += signals.binary.at(end) - signals.binary.at(start);
        speech_matched += signals.speech.at(end) - signals.speech.at(start);
        let target = start.saturating_add(lead);
        let before = target.saturating_sub(frame);
        let after = target.saturating_add(frame);
        let preceding = if before < target {
            signals.binary.fraction(before, target)
        } else {
            0.0
        };
        onset += (signals.binary.fraction(target, after) - preceding).max(0.0);
    }
    let bin_frac = binary_matched / duration;
    let prob_frac = speech_matched / duration;
    let onset_frac = onset / indices.len() as f64;
    (
        0.85 * bin_frac + 0.15 * prob_frac + 0.06 * onset_frac,
        bin_frac,
        onset_frac,
    )
}

pub(super) fn snap_join_limit(plan: &TimingPlan, g: usize, previous_end: u64) -> u64 {
    g.checked_sub(1)
        .and_then(|prev| joined_gap(plan.groups[prev].end_ms, plan.groups[g].start_ms))
        .map_or(u64::MAX, |gap| previous_end.saturating_add(gap + 80))
}

pub(super) fn snap_moves(before: u64, candidate: u64) -> bool {
    candidate > 0 && (candidate < before || candidate > before.saturating_add(20))
}

pub(super) fn snap_start(
    cue: &mut super::SubtitleCue,
    min_start: u64,
    vad: &VadResult,
    signals: &CueIntegrals<'_>,
    config: &SyncConfig,
    max_start: u64,
    unit: u64,
) -> bool {
    let distance = config.snap_ms;
    if distance == 0 || cue.end_ms - cue.start_ms < 500 {
        return false;
    }
    let low = cue.start_ms.saturating_sub(distance);
    let high = cue.start_ms.saturating_add(distance);
    let lead = config.lead_in_ms;
    let baseline = signals.binary.fraction(cue.start_ms, cue.end_ms);
    let first = vad.segments.partition_point(|span| span.start_ms < low);
    // A formatted anchor can precede confirmation by the lookback and rounding allowance.
    let search_high = high
        .saturating_add(lead)
        .saturating_add(unit - 1)
        .saturating_add(MAX_ONSET_LOOKBACK_MS);
    let allowed = |start, seg_start| {
        start >= low
            && start <= high
            && start >= min_start
            && start <= max_start
            && start <= cue.end_ms - 500
            && start > 0
            && snap_candidate_allowed(*cue, seg_start, start, lead, signals)
    };
    let candidate = vad.segments[first..]
        .iter()
        .enumerate()
        .take_while(|(_, span)| span.start_ms <= search_high)
        // Score the confirmed onset, not silence in the lead-in or consonant closure.
        .filter(|(_, span)| {
            span.start_ms < cue.end_ms
                && signals.binary.fraction(span.start_ms, cue.end_ms) + 1e-9 >= baseline
        })
        .filter_map(|(i, span)| {
            let before_onset = |onset: u64| onset.saturating_sub(lead) / unit * unit;
            vad.onset_anchors_ms
                .get(first + i)
                .map(|&onset| before_onset(onset))
                .filter(|&start| allowed(start, span.start_ms))
                .or_else(|| {
                    let start = before_onset(span.start_ms);
                    allowed(start, span.start_ms).then_some(start)
                })
        })
        .min_by_key(|&start| start.abs_diff(cue.start_ms))
        .filter(|&start| snap_moves(cue.start_ms, start));
    if let Some(start) = candidate {
        cue.start_ms = start;
        true
    } else {
        false
    }
}

fn snap_candidate_allowed(
    cue: super::SubtitleCue,
    seg_start: u64,
    candidate: u64,
    lead: u64,
    signals: &CueIntegrals<'_>,
) -> bool {
    if candidate < cue.start_ms && cue.start_ms - candidate > 80 && cue.start_ms >= seg_start {
        let span_start = candidate.saturating_add(lead).min(cue.start_ms);
        if span_start < cue.start_ms {
            if signals.voiced_only.fraction(span_start, cue.start_ms) < 0.20 {
                return false;
            }
            let pre_p = signals.speech.fraction(span_start, cue.start_ms);
            let in_p = signals
                .speech
                .fraction(
                    cue.start_ms,
                    cue.start_ms.saturating_add(120).min(cue.end_ms),
                )
                .max(signals.speech.fraction(
                    cue.start_ms,
                    cue.start_ms.saturating_add(300).min(cue.end_ms),
                ));
            if in_p >= 0.45 && pre_p + 0.15 < in_p {
                return false;
            }
        }
    }
    !(candidate > cue.start_ms.saturating_add(100)
        && signals.voiced_union.fraction(cue.start_ms, candidate) >= 0.50)
}

const TRAIL_OUT_MS: u64 = 250;
const MIN_READABLE_MS: u64 = 900;
const MIN_GAP_MS: u64 = 20;

pub(super) fn end_bounds(
    plan: &TimingPlan,
    g: usize,
    cues: &[super::SubtitleCue],
    duration: u64,
) -> (u64, u64) {
    let Some(next) = plan.groups.get(g + 1) else {
        return (0, duration);
    };
    let start = cues[plan.order[next.first]].start_ms;
    match joined_gap(plan.groups[g].end_ms, next.start_ms) {
        Some(gap) => (start.saturating_sub(gap + 80), start.min(duration)),
        None => (0, start.saturating_sub(MIN_GAP_MS).min(duration)),
    }
}

fn continuation_target(vad: &super::VadResult, end: u64, merged_end: u64) -> Option<u64> {
    let islands = &vad.raw_segments;
    if islands.is_empty() {
        return Some(merged_end.saturating_add(TRAIL_OUT_MS));
    }
    let next = islands.partition_point(|island| island.start_ms <= end);
    let island = islands.get(next.checked_sub(1)?)?;
    if end.saturating_sub(island.start_ms) < 100 || island.end_ms.saturating_sub(end) < 70 {
        return None;
    }
    let target = island.end_ms.saturating_add(TRAIL_OUT_MS);
    Some(islands.get(next).map_or(target, |next| {
        target.min(next.start_ms.saturating_sub(MIN_GAP_MS))
    }))
}

pub(super) fn refine_ends(
    plan: &TimingPlan,
    cues: &mut [super::SubtitleCue],
    vad: &super::VadResult,
    voiced_union: &Integral<'_, u8>,
    limit: u64,
) -> Result<usize, SyncError> {
    let mut changed = 0;
    for (g, group) in plan.groups.iter().enumerate() {
        check_cancelled()?;
        if group.end - group.first != 1 {
            continue;
        }
        let i = plan.order[group.first];
        let cue = cues[i];
        let (min_end, max_end) = end_bounds(plan, g, cues, vad.duration_ms);
        let low = cue
            .end_ms
            .saturating_sub(limit)
            .max(cue.start_ms + 500)
            .max(min_end)
            .div_ceil(plan.unit)
            * plan.unit;
        let high = cue.end_ms.saturating_add(limit).min(max_end) / plan.unit * plan.unit;
        if low > high {
            continue;
        }
        let split = vad
            .segments
            .partition_point(|seg| seg.start_ms < cue.end_ms);
        let last = split.checked_sub(1).map(|i| &vad.segments[i]);
        let duration = cue.end_ms - cue.start_ms;
        let original = plan.base[i];
        let original_duration = original.end_ms - original.start_ms;
        let target =
            if let Some(seg) = last.filter(|seg| seg.end_ms > cue.end_ms.saturating_add(140)) {
                if high <= cue.end_ms {
                    continue;
                }
                let Some(target) = continuation_target(vad, cue.end_ms, seg.end_ms) else {
                    continue;
                };
                if !vad.raw_segments.is_empty() && target < low {
                    continue;
                }
                target.clamp(low, high)
            } else if original_duration < MIN_READABLE_MS
                && duration < MIN_READABLE_MS
                && high > cue.end_ms
            {
                let next_speech = vad.segments.get(split).map_or(high, |seg| seg.start_ms);
                let target = (cue.start_ms + MIN_READABLE_MS).min(next_speech).min(high);
                if target < low.max(cue.end_ms) {
                    continue;
                }
                target
            } else if let Some(seg) = last.filter(|seg| {
                duration > 1500 && seg.end_ms > cue.start_ms && cue.end_ms > seg.end_ms + 600
            }) {
                let tail_ms = cue.end_ms - seg.end_ms;
                if low > high.min(cue.end_ms)
                    || voiced_union.fraction(seg.end_ms, cue.end_ms) >= 0.15
                    || (tail_ms <= 750 && tail_ms.saturating_mul(4) <= duration)
                {
                    continue;
                }
                (seg.end_ms + TRAIL_OUT_MS)
                    .max(cue.start_ms + MIN_READABLE_MS)
                    .clamp(low, high.min(cue.end_ms))
            } else {
                continue;
            };
        let end = target / plan.unit * plan.unit;
        if end != cue.end_ms {
            cues[i].end_ms = end;
            changed += 1;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::{TimingPlan, refine_ends};
    use crate::{
        SubtitleCue, SubtitleFormat,
        vad::{TimeSpan, VadResult},
    };

    #[test]
    fn ongoing_speech_only_extends_an_end_when_there_is_room() {
        let cue = |start_ms, end_ms| SubtitleCue { start_ms, end_ms };
        let authored = [cue(1000, 2000), cue(2300, 3700)];
        let vad = VadResult {
            raw_segments: Vec::new(),
            onset_anchors_ms: Vec::new(),
            voiced_activity: Vec::new(),
            frame_ms: 10,
            duration_ms: 4000,
            probability: Vec::new(),
            activity: Vec::new(),
            segments: vec![TimeSpan {
                start_ms: 1000,
                end_ms: 3100,
            }],
        };
        let voiced_union = super::Integral::new(&vad.activity, &vad);
        for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
            let plan = TimingPlan::new(&authored, format, 1.0);
            for (next_start, expected) in [(2000, 2000), (2020, 2000), (2040, 2020)] {
                let mut transformed = [authored[0], cue(next_start, next_start + 1400)];
                let changed =
                    refine_ends(&plan, &mut transformed, &vad, &voiced_union, 600).unwrap();
                assert_eq!(
                    transformed[0].end_ms, expected,
                    "{format:?} next={next_start}"
                );
                assert_eq!(changed, usize::from(expected != 2000));
                assert_eq!(transformed[1], cue(next_start, next_start + 1400));
            }
        }
    }
}
