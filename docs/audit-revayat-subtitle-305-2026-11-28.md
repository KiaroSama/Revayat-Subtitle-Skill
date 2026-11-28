# Revayat Subtitle: 3.0.5 source recheck

Review date: **2026-11-28, Asia/Tehran**.

## Revision and baseline

This recheck pins main at `8293a4f6d04d2760f1815b94f7430aed91d29381`, tree
`bc7c4330b59dc4c61150d019c8a16c6b0cd9c086`. The owner had merged
[PR #43](https://github.com/KiaroSama/Revayat-Subtitle-Skill/pull/43), then changed
binary speech alignment, covered-window locks, scene-adaptive voiced activity
and recovered sibilant handling. Earlier repairs remain regression controls.
The accepted scope is two new audio defects and one factual README correction.

Untouched baseline: release build, formatting and Clippy with warnings denied
pass. The complete workspace with `REVAYAT_REQUIRE_FFMPEG=1` and
`--no-fail-fast` passes **1286 tests, 0 failed, 0 ignored**, across 66 summary
rows. An earlier local attempt ended incompletely under disk pressure; the
successful repeat disabled dev/test debug information to fit the environment.
It did not skip tests or disable FFmpeg.

Actual baseline [CI run 46096879078](https://github.com/KiaroSama/Revayat-Subtitle-Skill/actions/runs/46096879078)
passes on that exact SHA. All 11 baseline checks succeed:

| Native Rust target | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| Linux x64 | 1286 | 0 | 0 |
| Linux ARM64 | 1286 | 0 | 0 |
| Windows x64 | 1254 | 0 | 0 |
| Windows ARM64 | 1254 | 0 | 0 |

Windows ARM64 uses native ARM64 Rust and the workflow's verified x64 FFmpeg
under Windows emulation. Baseline results do not certify the repair head.

## R305-01: high coverage locks genuinely late cues

**Impact:** a dialogue cue can remain hundreds of milliseconds late despite
having a clear earlier onset inside its local search allowance. Joined cues
and shorter cues with detector tail padding are affected too.

The new `already_covered` guard in `align_cue::refine_group_shifts` blocks the
candidate loop when the opening contains voiced activity and enough of the
authored window overlaps primary speech. A cue can satisfy those conditions
while missing its own phrase's beginning. The lock runs before onset evidence
and binary-coverage improvement can establish a safe correction.

### Actual reproduction and comparison

Generate 15 seconds of 16 kHz mono PCM with 400, 1200 and 2400 Hz tones at
amplitude 3500 each in `1000..2000`, `5000..7000`, `11000..12000` ms, otherwise
silence. Author matching cues except the middle one is `5550..7550` ms. Disable
global/FPS search, allow a 1500 ms local shift and retain default 350 ms snapping
and 600 ms end adjustment. Baseline leaves the middle cue **550 ms late**.
The JSON report remains internally consistent; this is an accuracy defect.

The public matrix exercises six scenarios, SRT/ASS and three backends:
standalone `subsync`, in-process `revayat-subtitle sync` and supervised
`revayat-subtitle sync --timeout 30`. All 36 baseline processes succeed but
24 fail the independently authored onset expectation. Every source/audio hash
and untimed payload is preserved; backend bytes and normalized reports agree.

The historical implementation `c660eb8f3abae0fdd759fd8f6ced7101c7647ab0` passes
12/12 standalone comparisons. Repaired binaries pass **36/36**. All 24 former
failures recover; all 12 control rows keep exactly their baseline timings and
normalized reports. The four restored scenarios match historical results in
both formats, eight historical/repaired pairs:

| Scenario | Broken baseline | Historical and repaired |
| --- | --- | --- |
| Late long cue, default endpoints | `5550..7550` | `4940..7260` |
| Late long cue, endpoints disabled | `5550..7550` | `4940..6940` |
| Late short cue | `5550..6930` | `4940..6640` |
| Late joined pair | `5550..6580`, `6580..7580` | `4940..5970`, `5970..7290` |

### Narrow repair and regressions

Locate the primary segment containing the current first-cue start. Keep the
anchored-cue lock. For an otherwise covered group, admit only a backward
candidate whose lead-adjusted start is within one frame of that same segment's
onset. Then apply the existing binary gain, onset evidence, score improvement,
distance penalty, search limits and neighboring-group bounds. Do not remove
covered-window protection globally or score by loudness again.

`sync_covered_drift.rs` adds seven permanent real-PCM tests: long, short and
joined late recovery with default/disabled endpoints, a disabled local limit,
early recovery, a continuous louder wall and an anchored cue resisting a louder
neighbor. Both formats undergo serialization, untimed-content preservation,
no-new-overlap checks and `validate_reported_timing` replay.

The first three tests fail on baseline. An initial control fixture also shifted
100 ms because its outer windows rewarded existing VAD trailing padding.
Extending only those outer reading tails makes them stable; the middle cue and
its assertion are unchanged. Corrected controls pass on frozen baseline in both
formats. That oracle correction is not another product defect.

## R305-02: recovered onset evidence is discarded during snapping

**Impact:** the detector finds an earlier consonant or quiet voiced attack but
the final subtitle can miss 250-290 ms of it. Moving the authored start only
10 ms can determine whether valid recovery survives.

Use seven seconds of 16 kHz mono PCM with 5200 Hz/amplitude 1400 plus
6400 Hz/amplitude 1000 in `2050..2300` ms, then the confirmed speech tones above
in `2300..3400` ms. The detector confirms speech at 2300 ms and separately
recovers the attack at 2050 ms. With default 350 ms snapping, 50 ms lead-in,
global/local offsets zero, FPS search off and end adjustment zero:

| Authored start | Correct target | Baseline SRT/ASS |
| ---: | ---: | ---: |
| 2200 | 2000 | 2000 |
| 2290 | 2000 | 2000 |
| 2300 | 2000 | 2250 |
| 2350 | 2000 | 2250 |

The new backward-snap guard mistakes recovered fricative evidence for ordinary
hiss because its voiced-only coverage is zero. Previously supported low
harmonic attacks pass the voiced test but fail the probability comparison:
alignment masks probability outside primary activity, so pre-confirmation score
is zero despite separately corroborated voiced activity.

### Narrow repair and regressions

Carry explicit recovery provenance from `onset < span.start_ms` into candidate
validation. Bypass only the backward voiced/probability vetoes for that recovered
candidate. Primary and fallback primary candidates still run those guards.
Do not infer recovery from a rounded timestamp plus lead-in. Keep confirmed
overlap, exact movement, positive start, neighbor bounds, minimum 500 ms
duration, ASS precision, forward voiced protection and public VAD validation.

The frozen matrix has **31 waveforms and 132 SRT/ASS cases**, including
**40 baseline failures**, at 11025, 16000, 22050, 44100, 48000, 96000 and
192000 Hz. Controls include clipped/continuous hiss, loud preceding background,
long gaps, lookback boundaries, quiet/deep tails and silence trimming. All
baseline processes exit successfully and preserve sources. Separately,
11 malformed/fallback vector checks and 54 rate/frame shape boundaries pass.

The repaired matrix passes **132/132**: all 40 failures recover and the other
92 cases preserve expected behavior. All 31 complete detector results match
baseline, including arrays and anchors; source/audio fixture hashes match too.
The fixed probe passes the same 11 vector and 54 shape checks. Sixteen additional
in-process/supervised runs match standalone bytes and normalized reports.
Two standalone references use a 450 ms radius for a 430 ms lookback move; the
default 350 ms radius correctly refuses that out-of-range target.

`sync_recovered_onsets.rs` adds five real-PCM tests with 87 sync/replay checks:
fricative and quiet/deep attacks, exact snap bounds, previous-cue equality,
minimum reading time, disabled snapping, overlapping ASS groups, false onsets,
lookback boundaries and three representative rates. Four tests fail on baseline.

## Verification and closure

The first focused fixed run passes **51 tests, 0 failed, 0 ignored**, including
both new targets and prior onset search, snapping, end adjustment and replay
regressions. Independent review finds no actionable defect in the narrow
production changes and confirms preservation of the remaining guards.

The complete fixed local gates also pass: optimized workspace build, formatting,
Clippy with warnings denied and **1298 workspace tests, 0 failed, 0 ignored**,
across 68 summary rows, with required FFmpeg/libass and `--no-fail-fast`.

Optimized fixed binary SHA-256:

| Binary | SHA-256 |
| --- | --- |
| `subsync` | `f0503fec97d0a6510d06fd7ddb06c549eaf5476a7a6936977595f731a1d17afb` |
| `revayat-subtitle` | `9deb27eaa0c78e2d6bb73d14dc64aaac09a570491b4a676a20dd4990a10cf91b` |

The repair PR records complete local gates and actual native CI logs on its
final head. All new tests run through the existing four-host workspace command;
no duplicate workflow or dependency is needed. The English README now correctly
describes all four native test targets, as the Persian README already does.

Only sync selection changes. Reviewed-edition generation stays `20` and
normalization `17`: build output from the same reviewed inputs is unchanged.
Synced files have their own changed source identities. Existing migration,
visual-evidence, installer and no-clobber suites remain regression controls.

At intake, source/plugin metadata and annotated tag `v3.0.5` match pinned main.
Latest published downloadable assets remain `v3.0.4`; a tag is not a published
asset receipt. About and all 20 topics already describe Rust/subtitles and the
dynamic badges remain connected to actual tags.

One historical human-author email exception remains at
`95ad08b1027adb70bda775e95522ff7c62f8e518`. Rewriting an ancestor requires the
owner's authorized history-maintenance procedure; a code PR does not fix it.
The auditor does not merge/close PRs, move tags or rewrite main.

Stop this accepted scope after the reproduced failures are fixed, controls
remain valid and final-head gates pass. Authored signals verify timing mechanics
and integration, not speech semantics for every film. Playback review of actual
supplied dialogue remains necessary.

## Integration disposition (2026-11-28)

GitHub published signed release `v3.0.5` from `8293a4f` at 11:46 UTC on
2026-11-28 with 16 signed assets. PR #44 follows it under `Unreleased`.
No new release tag or local build is planned unless requested.

The owner chose to keep `95ad08b1027adb70bda775e95522ff7c62f8e518` and the
published signed tags `v2.0.0..v3.0.5` without rewriting history. The commit
uses a GitHub no-reply alias, not a private personal email. New commits use
the repository's approved author and committer address.

We removed the stale `crates/subsync-rs/Cargo.lock`. Candidate diagnostics
and the consented speech benchmark are deferred. GPU acceleration is deferred
until profiling shows where it would help.

To address CodeRabbit finding `4727324372` on PR #44, the
[audio-sync reference](../skills/revayat-subtitle/references/audio-sync.md)
now documents group-refinement bounds separately from start-snapping bounds.

## Research references

- [FFsubsync](https://github.com/smacke/ffsubsync#how-it-works): binary speech
  correlation and limitations; it does not identify which speaker owns a cue.
- [ALASS](https://github.com/kaegi/alass): split-penalty alignment principle;
  this repair keeps existing search/penalty settings.
- [Cargo test](https://doc.rust-lang.org/cargo/commands/cargo-test.html):
  workspace tests and `--no-fail-fast` evidence collection.
- [Spec Kit chain](https://github.github.com/spec-kit/reference/agentic-sdd.html):
  follow the owner's configured Rules and invocation syntax. This checkout has
  no configured Spec Kit development-command tree or active custom hooks.
