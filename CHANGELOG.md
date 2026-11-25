# Changelog

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
