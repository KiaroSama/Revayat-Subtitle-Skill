use super::{Integral, SyncConfig, SyncError, TimingPlan, check_cancelled};

pub(super) fn refine_group_shifts(
    plan: &TimingPlan,
    speech: &Integral<'_, f32>,
    global_shift: i64,
    config: &SyncConfig,
    offsets: &mut [i64],
) -> Result<(), SyncError> {
    let limit = config.max_split_offset_ms;
    let radius = limit.min(2_000);
    let frame = speech.frame_ms as i64;
    let mut previous_end = 0;
    for (i, group) in plan.groups.iter().enumerate() {
        check_cancelled()?;
        let base = offsets[i];
        let next_start = plan.groups.get(i + 1).map_or(speech.duration_ms, |next| {
            next.start_ms.saturating_add_signed(offsets[i + 1])
        });
        let low = (base - radius)
            .max(global_shift - limit)
            .max(previous_end as i64 - group.start_ms as i64);
        let high = (base + radius)
            .min(global_shift + limit)
            .min(next_start as i64 - group.end_ms as i64);
        let mut best = base;
        // Leave pre-existing macro conflicts to the same repair used during replay.
        if (low..=high).contains(&base) {
            let indices = &plan.order[group.first..group.end];
            let duration: u64 = indices
                .iter()
                .map(|&index| plan.base[index].end_ms - plan.base[index].start_ms)
                .sum();
            let score = |shift| {
                let mut matched = 0.0;
                let mut onset = 0.0;
                for &index in indices {
                    let cue = plan.base[index];
                    let start = cue.start_ms.saturating_add_signed(shift);
                    let end = cue.end_ms.saturating_add_signed(shift);
                    matched += speech.at(end) - speech.at(start);
                    let before = start.saturating_sub(frame as u64);
                    let after = end.min(start + frame as u64);
                    let preceding = if before < start {
                        speech.fraction(before, start)
                    } else {
                        0.0
                    };
                    onset += (speech.fraction(start, after) - preceding).max(0.0);
                }
                matched / duration as f64 + 0.005 * onset / indices.len() as f64
            };
            let base_score = score(base);
            let mut best_score = base_score;
            for step in -((base - low) / frame)..=(high - base) / frame {
                check_cancelled()?;
                let candidate = base + step * frame;
                let value = score(candidate);
                if value > base_score + 0.04
                    && (value > best_score + 1e-9
                        || (value - best_score).abs() <= 1e-9
                            && candidate.abs_diff(base) < best.abs_diff(base))
                {
                    best = candidate;
                    best_score = value;
                }
            }
        }
        offsets[i] = best;
        let actual = if plan.format == super::SubtitleFormat::Ass {
            best.max(previous_end as i64 - group.start_ms as i64)
        } else {
            best.max(-(group.start_ms as i64))
        };
        previous_end = group.end_ms.saturating_add_signed(actual);
    }
    Ok(())
}
