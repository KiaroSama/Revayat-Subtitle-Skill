<div align="center">

# Revayat Subtitle (روایت زیرنویس)

**Translate, edit, and audio-sync subtitles into reviewed ASS (default) or SRT deliverables in `Sub.zip`. Persian is the default output language.**

[![GitHub Actions CI workflow with manual dispatch](https://img.shields.io/badge/CI-GitHub%20Actions-16a34a)](.github/workflows/ci.yml)
[![GNU General Public License v3.0 or later](https://img.shields.io/badge/license-GPL--3.0--or--later-2563eb)](LICENSE)
[![Release tag v2.0.0 across five plugin formats](https://img.shields.io/badge/release-v2.0.0-ea580c)](https://github.com/KiaroSama/Revayat-Subtitle-Skill/releases/tag/v2.0.0)
[![Automated pytest and cargo test suites across CI matrices](https://img.shields.io/badge/tests-pytest%20%7C%20cargo-0d9488)](.github/workflows/ci.yml)
[![Python 3.13.15 measured locally, requires Python 3.11 or newer](https://img.shields.io/badge/Python-3.13.15-8b5cf6)](docs/platforms.md)
[![Rust 1.100.0 measured locally for audio sync and PyO3 acceleration crates](https://img.shields.io/badge/Rust-1.100.0-9a3412)](rust/subsync-rs/Cargo.toml)
[![Runs on Windows, Linux, and macOS](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-0891b2)](docs/platforms.md)
[![Install via five plugin formats or standalone scripts](https://img.shields.io/badge/install-5%20plugins%20%7C%20standalone-4f46e5)](#install)
[![Documentation in English and Persian](https://img.shields.io/badge/docs-English%20%7C%20Persian-0284c7)](README.fa.md)
[![Published Git tree size measured via git ls-files is 1.71 MB](https://img.shields.io/badge/repo%20size-1.71%20MB-ca8a04)](#repository-size)
[![ASS default output with bundled B Koodak font and Translator Mode](https://img.shields.io/badge/output-ASS%20%7C%20B%20Koodak-15803d)](skills/revayat-subtitle/references/subtitle-formats.md)
[![Rust FFT and ALASS audio-to-subtitle synchronization engine](https://img.shields.io/badge/audio%20sync-Rust%20FFT%20%7C%20ALASS-b45309)](skills/revayat-subtitle/references/audio-sync.md)
[![FFmpeg and libass visual verification pipeline](https://img.shields.io/badge/renderer-FFmpeg%20%7C%20libass-334155)](skills/revayat-subtitle/references/persian-typography.md)
[![Three bundled voice guides for Persian, English, and Russian](https://img.shields.io/badge/voice%20guides-3%20bundled-7c3aed)](skills/revayat-subtitle/references/target-voice.md)
[![CodeQL static analysis and Gitleaks secret scanning](https://img.shields.io/badge/security-CodeQL%20%7C%20Gitleaks-c2410c)](.github/workflows/codeql.yml)
[![Support donations](https://img.shields.io/badge/Support-donations-d04a9a)](#donate)

</div>

<a id="repository-size"></a>

Published Git tree size measured from `git ls-files` (v2.0.0): 1,711,452 bytes (1.71 MB) across 219 tracked files, excluding Git history and untracked files.

An agent skill for Claude Code, Codex, Cursor, Antigravity, Kiro, Cline, Hermes,
OpenCode, and any coding agent that reads a `SKILL.md`. It handles subtitle work
that fails in naive machine translation: accounting for every cue across multiple
releases, keeping character names and honorifics consistent across seasons,
protecting ASS effects, rendering clean Persian ASS with bundled `B Koodak`,
anchoring RTL punctuation across line wraps, building Translator Mode files, and
synchronizing subtitle timing directly to video audio in Rust.

<div align="left"><a href="LICENSE">GPL-3.0 licensed</a></div>
<div align="right"><a href="README.fa.md">فارسی</a></div>

---

## What it handles

| Capability | How it works |
| --- | --- |
| ASS by default with bundled `B Koodak` | Outputs 1080p ASS (`PlayResX: 1920`, `PlayResY: 1080`) using bundled `BKOODB.TTF` (`B Koodak` Bold) with a subtle black outline (`1.5`) and soft shadow (`0.8`), overridable via CLI flags. SRT remains available on request (`--output-format srt`). |
| RTL punctuation and balanced wrapping | Wraps long lines to balanced 2-line `\N` breaks (42 visible chars per line) without splitting ASS tags or ZWNJ words, and wraps each display line with `RLE` (`U+202B`), trailing `RLM` (`U+200F`), and `PDF` (`U+202C`) so periods, question marks, and quotes never jump to line 1 in VLC, mpv, PotPlayer, or libass. |
| Persian ZWNJ and watermark removal | Enforces Persian ZWNJ (نیم‌فاصله) across `می‌`/`نمی‌`, `‌ها`, `‌تر`/`‌ترین`, and `ه‌` enclitics, converts Arabic `ي`/`ك`/`ة`/`ى` to Persian forms outside tags and URLs, and strips fansub, studio, Telegram, and URL credits automatically with a structured removal log. |
| Preserved italics, bold, and effects | Converts SRT `<i>`, `<b>`, `<u>`, `<s>`, `<font color>`, and `{an1..9}` into ASS override tags (`{\i1}`, `{\b1}`, `{\c&HBBGGRR&}`, `{\an8}`), while keeping existing vector drawings, positioning, karaoke, and layered ASS effects intact. |
| Translator Mode (`translator-ass`) | Merges a primary release with secondary donor releases in chronological order, inserting empty timed `Dialogue:` placeholder slots for non-overlapping donor cues and optional `Comment:` source reference lines. |
| Rust audio-to-subtitle sync (`subsync-rs`) | Extracts 16 kHz mono audio via FFmpeg, runs dual-feature FFT voice activity detection, solves global offset and PAL/NTSC FPS drift with `rustfft` cross-correlation, detects mid-episode recap/ad cuts with ALASS split-penalty DP, and snaps cue edges to speech boundaries. Exposed as the sibling `revayat-subtitle-sync` skill and `revayat-subtitle.py sync`. |
| Rust PyO3 acceleration crates | Heavy text, Persian lint, and markup paths are ported to pure Rust (`rust/subcore-rs`, `rust/persian-lint-rs`, `rust/markup-rs`) with `#![forbid(unsafe_code)]`, behavioral parity verification via `rust-anything`, and per-call Python fallback (`REVAYAT_DISABLE_RUST=1`). |
| Multi-release editorial tracking | Stable cue IDs, source hashes, locked glossaries, and complete worksheets keep character names, Japanese honorifics, and every cue accounted for across seasons. |
| Visual FFmpeg + libass verification | Renders PNG frames using the bundled font directory (`:fontsdir=fonts`) so the agent inspects real glyphs, line balance, clipping, and mixed-script order before `qa` and `package` produce `Sub.zip`. |

## Install

Requires **Python 3.11 or newer** and FFmpeg built with libass. Python runtime
scripts use only the standard library; the bundled `B Koodak` Bold font lives at
`skills/revayat-subtitle/assets/fonts/BKOODB.TTF` (see its
[font notice](skills/revayat-subtitle/assets/fonts/NOTICE.txt)). Rust `1.100.0`
is only needed to build the native audio-sync binary or optional PyO3 extensions
locally.

```bash
git clone https://github.com/KiaroSama/Revayat-Subtitle-Skill.git
cd Revayat-Subtitle-Skill
python -m pip install -r skills/revayat-subtitle/requirements.txt
```

Install both `revayat-subtitle` and `revayat-subtitle-sync` into your agents:

```bash
# macOS / Linux
bash install/install.sh
```

```powershell
# Windows
./install/install.ps1
```

By default the installer targets existing user directories. Pass `--agent codex`
for a single agent, `--scope project --path PATH` for a project install,
`--dry-run` to preview destinations, or `--force` to back up before replacing.
Both launchers share `install/install.py`. Recovery journals and backups live
outside skill discovery in `revayat-recovery/`, serialized by an OS lock under
`~/revayat-install-state/`.

| Agent | User skills | Project skills |
| --- | --- | --- |
| Claude Code | `~/.claude/skills/` | `.claude/skills/` |
| Codex | `~/.agents/skills/` | `.agents/skills/` |
| Cursor | `~/.cursor/skills/` | `.cursor/skills/` |
| Kiro | `~/.kiro/skills/` | `.kiro/skills/` |
| Cline | `~/.cline/skills/` | `.cline/skills/` |
| Hermes | `~/.hermes/skills/` | `.hermes/skills/` |
| OpenCode | `~/.config/opencode/skills/` | `.opencode/skills/` |
| Antigravity IDE | `~/.gemini/antigravity/skills/` | `.agents/skills/` |
| Antigravity CLI (`antigravity-cli`) | `~/.gemini/antigravity-cli/skills/` | `.agents/skills/` |

Reload the agent after installing. Full discovery paths and manifest schemas are
documented in [docs/platforms.md](docs/platforms.md).

### Plugin installation (five formats)

The repository ships all five plugin manifest formats on the same `2.0.0` tree:

- **Claude Code:** `.claude-plugin/plugin.json` and `.claude-plugin/marketplace.json`
- **Cursor:** `.cursor-plugin/plugin.json` and `.cursor-plugin/marketplace.json`
- **Codex:** `.codex-plugin/plugin.json` and `.agents/plugins/marketplace.json`
- **Antigravity:** `.antigravity-plugin/plugin.json`
- **Agent Plugins 1.0:** root `plugin.json`

Install inside Claude Code:

```text
/plugin marketplace add KiaroSama/Revayat-Subtitle-Skill
/plugin install revayat-subtitle@revayat-subtitle-marketplace
```

The plugin exposes `translate-subtitles`, `revayat-subtitle-resume`,
`revayat-subtitle-qa`, and `revayat-subtitle-sync` under the `revayat-subtitle`
namespace. To build a portable local plugin bundle:

```bash
python install/install.py --plugin --destination "PATH/revayat-subtitle" --recovery-dir "PATH/recovery"
```

### Build the Rust audio-sync binary (optional)

```bash
cargo build --release --manifest-path rust/subsync-rs/Cargo.toml
```

### Check the install

```bash
python skills/revayat-subtitle/scripts/revayat-subtitle.py doctor
```

`ready: true` confirms FFmpeg and libass are available. Pass `--ffmpeg PATH` or
set `REVAYAT_FFMPEG` when FFmpeg is outside `PATH`.

## Use

Ask the agent to translate, convert, build a Translator Mode ASS file, or sync a
subtitle to video:

> Use revayat-subtitle for these subtitles. Keep character names and Japanese
> honorifics consistent, write natural spoken Persian in ASS with B Koodak, strip
> watermarks, and sync the final file to episode01.mkv.

Persian is the default output language. Russian and English outputs route to the
[bundled voice guides](skills/revayat-subtitle/references/target-voice.md).

### Direct CLI commands

```bash
PY=python3
S=skills/revayat-subtitle/scripts

# 1. Environment check
"$PY" "$S/revayat-subtitle.py" doctor

# 2. Convert SRT to default Persian ASS (B Koodak, outline 1.5, shadow 0.8, ZWNJ, RTL wrap, watermark strip)
"$PY" "$S/revayat-subtitle.py" convert episode.srt --out episode.ass

# 3. Build a Translator Mode ASS from a primary release plus secondary donors
"$PY" "$S/revayat-subtitle.py" translator-ass french.srt --donor dutch.srt --out translator.ass

# 4. Audio-sync a subtitle directly to a video using Rust subsync-rs
"$PY" "$S/revayat-subtitle.py" sync episode.mkv episode.ass --out episode.synced.ass

# 5. Full season editorial workflow
"$PY" "$S/revayat-subtitle.py" prepare season.zip --work work/season01 --series "Anime title" --season 1
# Settle glossary.json, episode mapping in project.json, and worksheet decisions.
"$PY" "$S/revayat-subtitle.py" build --work work/season01
# Set BUILD to the build path printed above.
"$PY" "$S/revayat-subtitle.py" render --build "$BUILD" --episode S01E01 --video episode01.mkv
# Inspect every rendered PNG and record real observations in its review file.
"$PY" "$S/revayat-subtitle.py" qa --build "$BUILD"
"$PY" "$S/revayat-subtitle.py" package --build "$BUILD" --out out/Sub.zip
```

In PowerShell, set `$PY = 'python'` and `$S = 'skills/revayat-subtitle/scripts'`
and run `& $PY "$S/revayat-subtitle.py" ...`.

**Inputs:** ASS, SRT, folders, and ZIP archives (plus MKV/MP4/WAV for `sync`).
**Outputs:** ASS by default (`S01E01.ass`, `S01OVA01.ass`), or SRT when requested
via `--output-format srt`, packaged into `Sub.zip` alongside the updated glossary.

### Read-only resume and proposals

`status --work WORK` reports pending cues, glossary or assignment gaps, and
matching edition evidence. `handoff --work WORK --assignment FILE --result FILE`
checks disjoint, revision-bound subagent proposals without importing them. Both
emit version-1 JSON (exit `0` complete, `1` valid pending, `2` malformed or
stale). See
[workflow.md](skills/revayat-subtitle/references/workflow.md#read-only-status-and-editorial-handoffs)
for the full schema, plus the [rubric](evaluation/rubric.md) and
[pilot dataset](evaluation/pilot.json).

## How it works

```text
ASS / SRT / ZIP (+ optional MKV / MP4 / WAV)
       |
       v
prepare -> immutable sources + project.json + complete worksheets
       |                             |
       |                     research + locked glossary
       v                             |
compare releases <------------------+
       |
       v
agent translates and records every cue decision (ZWNJ, watermark filter, markup)
       |
       v
build -> ASS default (B Koodak, outline/shadow, balanced RTL wrap) or requested SRT
       |
       v
optional sync -> Rust subsync-rs (16 kHz FFT VAD + cross-correlation + ALASS DP)
       |
       v
render -> FFmpeg + libass PNGs (bundled fontsdir) -> agent visual inspection
       |
       v
qa -> source, edition, glossary, and image evidence must agree
       |
       v
package -> Sub.zip, read back and verified against the reviewed build
```

## What it is honest about

- The scripts enforce structure, hashes, ZWNJ rules, RTL marks, timing, and visual
  review artifacts. Translation fidelity and spoken fluency still depend on the
  model reading every cue in context.
- Audio sync (`subsync-rs`) aligns cues to speech activity in the soundtrack. It
  can recover global offsets, FPS drift, and mid-episode cuts, while heavy music
  or overlapping dubs may still benefit from a spot-check.
- `B Koodak` covers Persian letters, digits, punctuation, ZWNJ, and RLM; Latin
  runs fall back to the host system font via libass.

## Documentation

- [Main skill instructions](skills/revayat-subtitle/SKILL.md)
- [Audio sync skill instructions](skills/revayat-subtitle-sync/SKILL.md)
- [Audio sync reference](skills/revayat-subtitle/references/audio-sync.md)
- [Subtitle formats and Translator Mode](skills/revayat-subtitle/references/subtitle-formats.md)
- [Persian typography and RTL anchoring](skills/revayat-subtitle/references/persian-typography.md)
- [Workflow contracts](skills/revayat-subtitle/references/workflow.md)
- [Target voice guides](skills/revayat-subtitle/references/target-voice.md)
- [Source language rules](skills/revayat-subtitle/references/source-languages.md)
- [Platform paths and plugin manifests](docs/platforms.md)
- [Changelog](CHANGELOG.md)

## Development

```bash
python -m pip install -r requirements-dev.txt
python -X utf8 -m pytest tests/ -q
python -X utf8 tests/check.py --render
cargo test --manifest-path rust/subsync-rs/Cargo.toml
```

GitHub Actions CI (`.github/workflows/ci.yml`) runs on manual `workflow_dispatch`
across 16 jobs covering `actionlint`/`zizmor`, Python 3.11 and 3.14 on Linux,
Windows, and macOS, full pytest suites, five-format plugin validation, v2 ASS/RTL
pipeline tests, Rust `subsync-rs` (Linux and Windows), all three PyO3 Rust crates
(`subcore-rs`, `persian-lint-rs`, `markup-rs`), and repository hygiene metrics.
CodeQL and Gitleaks workflows scan code and Git history separately.

Execution logs record progress and errors under `logs/` without subtitle bodies.

## Donate

If this project helps you, donations are appreciated.

| Currency | Network | Address |
| --- | --- | --- |
| Bitcoin (BTC) | Bitcoin | `bc1qmth5m03pu5hujw5xw5jmywam3jj3sqwqupesdt` |
| USDT, BNB, USDC, etc. | BEP20 | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
| USDT, TRX, USDC, etc. | TRC20 | `TWBA3xFTqgZAeAYMxqo85xWnzvty3DcAhw` |
| Ethereum (ETH) | ERC20 | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
| TON | TON | `UQCN8Umo_OfOWqImZetQsrNStPcmLkMAKajFyiCOhso23NDb` |
| Litecoin (LTC) | LTC | `ltc1qntqnnrunadurnw4cshv3qgspywrueyyeyngwuy` |
| Solana (SOL) | Solana | `7B2wkczUjmkDhETwQuknBL8sUsbuV7nErxc317TmQuwR` |
| Polygon (POL) | Polygon | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |

GitHub Sponsors: [KiaroSama](https://github.com/sponsors/KiaroSama).

## Author

Author: Kiaro Sama
GitHub: [KiaroSama](https://github.com/KiaroSama)

## License

[GNU General Public License v3.0 or later](LICENSE). Copyright (C) 2026 Kiaro Sama.

This program is free software: you can redistribute it and/or modify it under
the terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. It is distributed in the hope that it will be useful, but WITHOUT ANY
WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A
PARTICULAR PURPOSE. See the license for details.

The bundled `B Koodak` Bold font (`skills/revayat-subtitle/assets/fonts/BKOODB.TTF`)
retains its original Borna Rayaneh attribution in
[NOTICE.txt](skills/revayat-subtitle/assets/fonts/NOTICE.txt). FFmpeg and user
media retain their own licenses and are not redistributed by this repository.
