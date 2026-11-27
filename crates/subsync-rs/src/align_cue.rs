use super::{Integral, SyncConfig, SyncError, TimingPlan, check_cancelled};

pub(super) fn refine_group_shifts(
    plan: &TimingPlan,
    speech: &Integral<'_, f32>,
    global_shift: i64,
    config: &SyncConfig,
    offsets: &mut [i64],
) -> Result<(), SyncError> {
    let groups = &plan.groups;
    let limit = config.max_split_offset_ms;
    let frame = speech.frame_ms as i64;
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
        let next_start = groups.get(end).map_or(speech.duration_ms, |next| {
            next.start_ms.saturating_add_signed(offsets[end])
        });
        let indices = &plan.order[groups[first].first..groups[end - 1].end];
        let score = |shift| score_shift(plan, speech, config.lead_in_ms, indices, shift);
        let baseline = score(base);
        let radius = limit.min(if baseline >= 0.35 { 900 } else { 1500 });
        let low = (global_shift - limit)
            .max(base - radius)
            .max(previous_end as i64 - groups[first].start_ms as i64);
        let high = (global_shift + limit)
            .min(base + radius)
            .min(next_start as i64 - groups[end - 1].end_ms as i64);
        let mut best = base;
        let mut best_score = baseline;
        if (low..=high).contains(&base) {
            for step in -((base - low) / frame)..=(high - base) / frame {
                check_cancelled()?;
                let candidate = base + step * frame;
                let value = score(candidate) - 0.00010 * candidate.abs_diff(base) as f64;
                if value > baseline + 0.05
                    && (value > best_score + 1e-9
                        || ((value - best_score).abs() <= 1e-9
                            && candidate.abs_diff(base) < best.abs_diff(base)))
                {
                    best = candidate;
                    best_score = value;
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
                let baseline = score_shift(plan, speech, config.lead_in_ms, indices, best);
                let mut top = baseline + 0.05;
                for step in -((best - lo) / frame)..=(hi - best) / frame {
                    check_cancelled()?;
                    let candidate = best + step * frame;
                    let value = score_shift(plan, speech, config.lead_in_ms, indices, candidate)
                        - 0.00004 * candidate.abs_diff(best) as f64;
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

fn score_shift(
    plan: &TimingPlan,
    speech: &Integral<'_, f32>,
    lead: u64,
    indices: &[usize],
    shift: i64,
) -> f64 {
    let frame = speech.frame_ms;
    let duration = indices
        .iter()
        .map(|&i| plan.base[i].end_ms - plan.base[i].start_ms)
        .sum::<u64>() as f64;
    let mut matched = 0.0;
    let mut onset = 0.0;
    for &i in indices {
        let cue = plan.base[i];
        let start = cue.start_ms.saturating_add_signed(shift);
        let end = cue.end_ms.saturating_add_signed(shift);
        matched += speech.at(end) - speech.at(start);
        let target = start.saturating_add(lead);
        let before = target.saturating_sub(frame);
        let after = target.saturating_add(frame);
        let preceding = if before < target {
            speech.fraction(before, target)
        } else {
            0.0
        };
        onset += (speech.fraction(target, after) - preceding).max(0.0);
    }
    matched / duration + 0.06 * onset / indices.len() as f64
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

pub(super) fn refine_ends(
    plan: &TimingPlan,
    cues: &mut [super::SubtitleCue],
    vad: &super::VadResult,
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
                // Continue toward ongoing speech even when the movement budget cannot cover it all.
                (seg.end_ms + TRAIL_OUT_MS).clamp(low, high)
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
                if low > high.min(cue.end_ms) {
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
