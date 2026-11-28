# Changelog

## Unreleased

- Recover late cues and joined dialogue toward the onset of their own speech segment even when most of the authored window already contains speech. Keep the guards against unrelated onset jumps and continuous-wall drift.
- Preserve separately recovered fricative and quiet/deep voiced onset evidence during backward snapping. Keep primary-onset hiss rejection, exact motion and neighbor limits, minimum reading time, format precision and worker replay checks.
- Add real-waveform regressions for both repairs and correct the English README's description of the four native CI test targets. See the [3.0.5 source recheck](docs/audit-revayat-subtitle-305-2026-11-28.md).
- Remove the stale `crates/subsync-rs/Cargo.lock` and clarify group-refinement and start-snapping limits in the [audio-sync reference](skills/revayat-subtitle/references/audio-sync.md).

## [3.0.5] - 2026-11-28

- Consider recovered speech onsets and ASS centisecond rounding throughout the complete permitted snap window. Keep exact endpoint, neighbor, overlap and replay guards.
- Reject onset candidates created by clipping a sustained background at the lookback or previous-segment boundary; retain real attacks at those boundaries.
- Resolve FSI from rendered ASS whitespace while preserving raw escapes, markup and directional controls. Respect header and inline wrapping modes during conversion and reviewed builds.
- Classify text direction from the visible ASS text so `\h` and soft `\n` no longer trigger an LTR wrapper on Persian-only lines.
- In ASS conversion, treat `\n` as a line break when `WrapStyle: 2`, an inline `\q2`, or `Banner` makes it one.
- Run Gitleaks in a separate concurrency group for each commit pushed to `main`, so a newer push does not cancel the earlier scan.
- Advance generation to `20` and normalization to `17` for the visible-ASS direction change. Rebuild recipe `19/16` workspaces into a distinct edition instead of reporting an untouched previous build as modified; preserve old files and require fresh render approval. See the [Rust 3.0.4 recheck](docs/audit-rust-304-2026-11-27.md), [issue #41](https://github.com/KiaroSama/Revayat-Subtitle-Skill/issues/41), and the [2026-11-28 edition migration recheck](docs/audit-revayat-subtitle-2026-11-28.md).
- Stop per-cue group refinement from sliding already-anchored dialogue cues across silence or through continuous loud scenes onto louder sound effects or neighboring lines, by scoring binary speech presence with onset-anchor and opening-voiced locks.
- Add scene-adaptive voiced activity (`120-3400 Hz`, 25 s local noise floor) and quiet-scene sibilant/fricative onset backtracking in `subsync-rs`; refuse backward snaps into unvoiced pre-speech hiss, refuse forward snaps over active voiced speech, and protect quiet or deep trailing voiced speech and short proportional reading tails from premature end trimming.

## [3.0.4] - 2026-11-27

- Check worker-reported start snaps against the cue end before refinement. Reject changed starts at zero and forward moves of 20 ms or less, as the engine does. Keep invalid worker output unpublished so a corrected run can retry (R01, H03).
- Extend a cue toward confirmed continuing speech even when that speech outlasts the adjustment budget. Check the cue’s own unpadded speech segment before extending, and leave its end unchanged if the next cue leaves no room to extend. Keep neighbor, audio-duration, and ASS timing bounds (R02, H02, S02).
- Look back from a confirmed onset to recover quiet, low-pitched starts. Use energy from 125 to 3400 Hz with upper-band evidence and a local noise floor to screen out isolated bass, drones, and flat music (S01).
- Keep valid sibling embeddings, and remove only unmatched bidi controls across the readable cue. Preserve balanced inner and cross-line scopes, markup, and drawings (R03, R04).
- Check the effective UAX #9 level before adding RTL or LTR wrappers. Respect the parity step and the first strong character in each FSI; use boundary marks if a wrapper would exceed level 125 (LTR-depth fix, H01).
- Use generation recipe `18` and normalization recipe `15`. Keep earlier editions and require fresh render evidence. See the [Rust 3.0.3 recheck](docs/audit-rust-303-2026-11-27.md) and [issue #39](https://github.com/KiaroSama/Revayat-Subtitle-Skill/issues/39).

## 3.0.3 (2026-11-27)

- Filter nearby speech onsets by overlap quality before snapping. When replaying worker results, check the implied onset after accounting for the configured lead-in (PR #38, A01–A02).
- Parse source ASS Default bold values as libass does. Expand abbreviated style schemas so generated columns survive conversion, including in mixed-source reviewed editions (PR #38, A03–A04).
- Use one FFmpeg deadline for probing, Center extraction, and mono fallback. Read FFprobe's structured JSON to detect Front Center (`FC`) without confusing it with Top Front Center (`TFC`) (PR #38, A05–A06).
- Align each cue's start and end to speech. Search farther for drift in standalone cues while keeping joined dialogue together. Extend ends that cut off speech, aim for 900 ms of readable duration when space allows, and trim excess silence toward a 250 ms trail-out. Bound end changes with `--end-adjust-ms` (default `600`) and validate the reported `cues_end_adjusted` count during replay.
- Wrap Persian RTL display lines in UAX #9 `RLM + RLI ... PDI` (`U+200F U+2067 ... U+2069`). Clean unbalanced bidi controls, replace intra-word `RLM` with ZWNJ, move reversed leading punctuation to the logical end, and keep commas from starting wrapped lines. Use generation recipe `17` and normalization recipe `14`.

## 3.0.2 (2026-11-26)

- Run Persian style and percent checks on inputs up to 16 MiB without exhausting the regex backtracking limit. Use a non-backtracking style matcher and a percent prepass that skips unrelated text (PR #36, F01).
- Score FPS drift using sparse maximum cue coverage beyond the FFT window. Duplicate and overlapping ASS rows no longer inflate tail energy (PR #36, F02). See the [post-merge Rust recheck](docs/audit-rust-postmerge-2026-11-25.md).
- Extract Front Center (`FC`) dialogue automatically on supported 5.1/7.1 tracks. Fall back to a mono downmix when the channel is unavailable or its RMS indicates silence.
- Keep dialogue chains with gaps of 150 ms or less together. Penalize distant timing shifts and target a 50 ms speech lead-in by default, adjustable with `--lead-in-ms`.
- Generate ASS with `B Koodak` at size `110`, outline and shadow `2`, and margins `10` on the left, right, and bottom. Default to non-bold, preserve a bold source ASS Default style, and allow `--bold`, `--no-bold`, and margin overrides. Bump the generation recipe to `15`.

## 3.0.1 (2026-11-25)

- Close the Rust audit's R01-R27 findings across subtitle processing, sync, installation, and output validation.
- Keep status checks working when a build staging directory disappears mid-scan. Fix Persian lint failures on long subtitle files by scanning single-line rules one line at a time.
- Score FPS drift against the full subtitle energy. Refine each dialogue cue's start and end against its own audio within neighboring cue bounds, then snap eligible starts to speech onsets.
- Validate marketplace versions against the release tag and require a passing full-history Gitleaks scan before building or signing release binaries.
- Improve Windows and Linux test reliability with isolated temporary directories for non-admin Windows runs and a bounded retry for Linux test launches that encounter `ETXTBSY`.
- Repair the Rust v3 subtitle pipeline's comment/override preservation, bidi scopes, SRT separator runs, generated styles, codec validation and literal JSON edition identities. Use generation recipe `14` and normalization recipe `13`; historical workspaces keep their sources and reviews while new editions require fresh image evidence.
- Preserve delayed/gapped audio timelines during sync, clean up owned processes on cancellation, bound standalone inputs, and replay external-worker endpoint transformations against the requested limits before publication.
- Correct automatic uninstall discovery, authenticated Unix executable ownership, early distribution budgets and Windows recovery path aliases. Keep legacy journal compatibility and safe retry behavior.
- Make diagnostic sink failures nonpanicking, log redacted failure causes, check supervised output delivery, preserve timeout/signal causes, and reject overflowing JSON integers and malformed public render timing.
- Recognize legitimate build staging in read-only status, apply PNG deadlines during incremental inflation, execute media tests on all four supported native Rust hosts, and require release tags to match package, manifest and binary versions.
- Document public adapter signature corrections and evidence in the [Rust v3 audit](docs/audit-rust-2026-11-25.md).

## 3.0.0 (2026-11-25)

### Runtime and distribution

- Move the runtime, installer and test suite to a pure Rust workspace using Rust `1.100.0`, edition `2024`. Ship no Python, PowerShell, or Bash scripts.
- Build two standalone binaries: `revayat-subtitle` with 16 subcommands, and `subsync` for aligning SRT and ASS to dialogue audio. Use `revayat-subtitle install` for installation and recovery. Copy local release outputs to `bin/`.
- Limit CI and releases to 64-bit Windows and Linux, each on x64 and arm64. CI builds both binaries for all four targets and runs workspace tests with FFmpeg and libass on x64 Windows and Linux. Media extraction and rendering still use external `ffmpeg` and `ffprobe`; rendering requires the libass `subtitles` filter.
- Update all five plugin manifest formats to `3.0.0`. Both READMEs use live badges for CI, license, latest published release, latest Git tag, and repository size.
- Use `GPL-3.0-or-later` across the Cargo workspace. Resolve symlinked video paths before rendering, sync new output directory entries on Unix, and skip regex compilation for glossary and idiom entries that cannot match.

### Subtitle workflow

- Document six sequential roles from initial translation to final delivery, with independent editorial and delivery reviews. Import `ffmpeg-skill` v4.7.1 guidance for checking audio tracks and timing. Update the bundled `humanizer` guide to v4.0.0 and retain the `persian` guide for natural phrasing and text mechanics.
- Keep Persian ASS and the bundled `B Koodak` font as defaults. Preserve literal HTML-looking ASS dialogue, overlapping ASS presentation order, and dialogue that mentions a link or subtitle service. Check reviewed links against the cues that survive normalization. Use generation recipe `13` and normalization recipe `12`; changed editions require fresh review.
- Bound malformed-comment scanning and preserve code spans and literal NUL characters during Persian linting.
- Handle CR, LF, CRLF, and mixed subtitle line endings. Refuse ambiguous SRT cue boundaries and audio without timing anchors. Stage complete sync outputs before publishing them to new paths. Refuse occupied package destinations, including dangling symlinks.
- Recover legacy installer registry overflow and authenticated interrupted transactions without discarding active records, journals, or backups. Cleanup retries leave later valid installations untouched.

## 2.0.0 (2026-11-24)

- Added all five plugin manifest formats matching the `cloudflare/skills` layout: Claude Code (`.claude-plugin/`), Cursor (`.cursor-plugin/` with marketplace), Codex (`.codex-plugin/` with `.agents/plugins/marketplace.json`), Antigravity (`.antigravity-plugin/`), and Agent Plugins 1.0.0 (`plugin.json`), with updated atomic multi-skill installation in `install/install.py`.
- Made ASS (`v4.00+`) the default output format with bundled `B Koodak` (`assets/fonts/BKOODB.TTF`) passed to FFmpeg/libass via `fontsdir`, subtle black outline (`1.5`) and shadow (`0.8`) by default (user-overridable), preserved italics/bold/alignment/colors/effects, tag-aware Persian ZWNJ enforcement, deterministic fansub/credit watermark removal, balanced 2-line `\N` wrapping (`<=42` visible chars/line), per-subline RTL + trailing `U+200F` punctuation anchoring, and chronological Translator Mode (`translator-ass` / `--translator-mode`).
- Ported the hot subtitle text/RTL/ZWNJ/wrapping/watermark/SRT-to-ASS pipeline (`rust/subcore-rs`), Persian typography linter (`rust/persian-lint-rs`), and subtitle markup/bidi core (`rust/markup-rs`) to pure Rust PyO3 crates via `rust-anything` through `gate`, and added a pure-Rust audio-to-subtitle synchronization engine and skill (`rust/subsync-rs`, `revayat-subtitle sync`, `skills/revayat-subtitle/sync/SKILL.md`, `skills/revayat-subtitle-sync/SKILL.md`, `commands/revayat-subtitle-sync.md`) combining FFmpeg 16 kHz PCM extraction, dual-feature VAD, FFT cross-correlation global/FPS sync, ALASS-style split-penalty DP alignment, and bounded per-cue boundary snapping.
- Expanded manual GitHub Actions CI (`ci.yml`) to 16 jobs covering workflow lint (`actionlint`/`zizmor`), Python 3.11/3.14 on Linux/Windows/macOS, full pytest suites, all five plugin manifests, v2 ASS/RTL pipeline tests, Rust `subsync-rs`, all three PyO3 Rust crates, Windows/Linux FFmpeg/libass rendering, and repository hygiene checks, and refreshed English/Persian documentation with 16 shields.io flat-square badges.

## 1.4.0 (2026-11-18)

- Revayat bundles Persian, Russian and English voice skills. Subtitle editors use the selected output language to choose the guide; Persian remains the default.
- The standalone installer and plugin ship the same guides, with pinned upstream sources and license notices.
- Project tests and workflow lint now run from one manual CI dispatch. CodeQL updates its `init` and `analyze` steps together.
