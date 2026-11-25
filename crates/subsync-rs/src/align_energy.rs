use super::{SubtitleCue, SyncError, check_cancelled};
use std::collections::BTreeMap;

pub(super) fn activity_tail_energy(
    cues: &[SubtitleCue],
    frames: u64,
    frame_ms: u32,
) -> Result<f64, SyncError> {
    tail_energy_with_check(cues, frames, frame_ms, check_cancelled)
}

fn tail_energy_with_check(
    cues: &[SubtitleCue],
    frames: u64,
    frame_ms: u32,
    mut checkpoint: impl FnMut() -> Result<(), SyncError>,
) -> Result<f64, SyncError> {
    checkpoint()?;
    let frame = u64::from(frame_ms);
    if frame == 0 {
        return Ok(0.0);
    }
    let boundary = frames.saturating_mul(frame);
    // Each cue contributes at most two fractional edges and one full-frame interval.
    // Ordered events avoid allocating or iterating over a distant subtitle tail.
    let mut events: BTreeMap<(u64, u32), i64> = BTreeMap::new();
    let mut add_interval = |start: u64, end: u64, covered: u64| {
        if start < end {
            *events.entry((start, covered as u32)).or_default() += 1;
            *events.entry((end, covered as u32)).or_default() -= 1;
        }
    };
    for cue in cues {
        checkpoint()?;
        let start = cue.start_ms.max(boundary);
        let end = cue.end_ms;
        if start >= end {
            continue;
        }
        let first = start / frame;
        let last = (end - 1) / frame;
        if first == last {
            add_interval(first, first + 1, end - start);
        } else {
            add_interval(first, first + 1, frame - start % frame);
            add_interval(first + 1, last, frame);
            add_interval(last, last + 1, (end - 1) % frame + 1);
        }
    }
    let mut active: BTreeMap<u32, i64> = BTreeMap::new();
    let mut previous = frames;
    let mut energy = 0.0;
    for ((index, covered), change) in events {
        checkpoint()?;
        if let Some((&maximum, _)) = active.last_key_value() {
            // Match the rasterizer's f32 maximum, including disjoint pieces in one frame.
            let value = f64::from(maximum as f32 / frame_ms as f32);
            energy += (index - previous) as f64 * value * value;
        }
        previous = index;
        let count = active.entry(covered).or_default();
        *count += change;
        if *count == 0 {
            active.remove(&covered);
        }
    }
    Ok(energy)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cues(spans: &[(u64, u64)]) -> Vec<SubtitleCue> {
        spans
            .iter()
            .map(|&(start_ms, end_ms)| SubtitleCue { start_ms, end_ms })
            .collect()
    }

    #[test]
    fn tail_energy_matches_raster_for_overlap_edges_and_clipping() {
        let cases: &[&[(u64, u64)]] = &[
            &[],
            &[(10, 13), (17, 20)],
            &[(10, 15), (15, 20)],
            &[(1, 7), (3, 9)],
            &[(5, 15), (12, 18)],
            &[(5, 35), (12, 28), (18, 22)],
            &[(5, 35), (15, 45), (5, 35)],
            &[(110, 170), (10, 29), (110, 170), (70, 30), (20, 20)],
        ];
        for spans in cases {
            let cues = cues(spans);
            for frame in [1, 3, 7, 10, 13, 100] {
                for boundary in [0, 1, 3, 10, 200] {
                    let raster = super::super::build_subtitle_activity(&cues, 200, frame);
                    let expected = raster[boundary..]
                        .iter()
                        .map(|&value| f64::from(value).powi(2))
                        .sum::<f64>();
                    let actual = activity_tail_energy(&cues, boundary as u64, frame).unwrap();
                    assert!(
                        (actual - expected).abs() <= 1e-10 * expected.max(1.0),
                        "spans={spans:?}, frame={frame}, boundary={boundary}: {actual} != {expected}"
                    );
                }
            }
        }
    }

    #[test]
    fn tail_energy_handles_twenty_four_hours_without_scanning_the_timeline() {
        for spans in [&[(1, 86_400_000)][..], &[(86_399_991, 86_400_000)][..]] {
            let mut checkpoints = 0;
            let energy = tail_energy_with_check(&cues(spans), 0, 10, || {
                checkpoints += 1;
                Ok(())
            })
            .unwrap();
            let expected = if spans[0].0 == 1 {
                8_639_999.0 + f64::from(0.9_f32).powi(2)
            } else {
                f64::from(0.9_f32).powi(2)
            };
            assert_eq!(energy, expected);
            assert!(checkpoints <= 8, "visited {checkpoints} checkpoints");
        }
    }

    #[test]
    fn tail_energy_can_cancel_during_collection_and_event_scan() {
        let input = cues(&[(11, 35), (72, 99)]);
        for stop in 1..=8 {
            let mut calls = 0;
            let result = tail_energy_with_check(&input, 0, 10, || {
                calls += 1;
                if calls == stop {
                    Err(SyncError::Cancelled)
                } else {
                    Ok(())
                }
            });
            assert!(matches!(result, Err(SyncError::Cancelled)), "stop={stop}");
            assert_eq!(calls, stop);
        }
    }

    #[test]
    fn zero_frame_has_zero_tail_energy() {
        assert_eq!(activity_tail_energy(&cues(&[(0, 100)]), 0, 0).unwrap(), 0.0);
    }
}
