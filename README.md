# Revayat Subtitle — روایت زیرنویس

[![CI](https://github.com/KiaroSama/Revayat-Subtitle-Skill/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/KiaroSama/Revayat-Subtitle-Skill/actions/workflows/ci.yml)
[![Python 3.10+](https://img.shields.io/badge/Python-3.10%2B-blue)](docs/platforms.md)
[![GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue)](LICENSE)

**Translate anime subtitles from any source language into spoken Persian, or repair Persian subtitles, and get one reviewed file per episode inside `Sub.zip`.**

An agent skill for Claude Code, Codex, Cursor, Kiro, Cline, Hermes, OpenCode,
Antigravity and any coding agent that can read a `SKILL.md`. It handles the parts
that need deliberate editorial care: every line in every release, names that stay
consistent between seasons, Japanese honorifics, effects that are not prose, and
Persian/English text that must display correctly in a real subtitle renderer.

<div align="left"><a href="LICENSE">GPL-3.0 licensed</a></div>
<div align="right"><a href="README.fa.md">فارسی</a></div>

---

## What it does that a generic subtitle translator does not

| | |
| --- | --- |
| **Every candidate is accounted for** | Stable cue IDs, source hashes and complete worksheets keep an omitted release or unread line from disappearing behind a successful build. |
| **Spoken Persian** | Any source language is translated and Persian is edited for spelling, meaning and natural conversation; the agent reads the whole dialogue in context. |
| **Names survive a sequel** | Anime research, source URLs and a reusable locked glossary carry names, places and character voice into later episodes and seasons. |
| **Japanese honorifics remain Japanese** | سان، کون، ساما، چان and اونی-چان stay forms of address rather than becoming آقا or خانم. |
| **One edition per episode** | A usable ASS release supplies presentation; better wording and missing content can come from other candidates without concatenating duplicate translations. |
| **RTL is inspected, not assumed** | Logical Unicode, real direction marks and FFmpeg images expose mixed-script order, punctuation, quote scope, missing glyphs and clipping. |
| **Effects remain effects** | Vector paths, positioning, animation, karaoke controls and intentional layers stay distinct from the signs and dialogue that need translation. |
| **Fonts are a choice** | When embedded fonts are found, the agent asks whether to retain or remove them unless the user already decided for this batch; the replacement font is checked after removal. |
| **Optional parallel editing** | The agent asks before assigning disjoint episodes or cue ranges to subagents. Shared names, separate worker logs and coordinator review keep the results consistent. |
| **Narrow cleanup** | Verified promotion, empty cues, hidden comments and unused styles are removed; story dialogue, profanity and sexual language retain their meaning and intensity. |
| **The delivery checks its own evidence** | `qa` and packaging use the same source, subtitle, glossary and image checks, and the finished ZIP is read back before success. |

## Install

**Python 3.10 or newer**, FFmpeg built with libass, and a Persian-capable font.
Python uses only its standard library; the requirements file declares that empty
Python dependency set. FFmpeg and fonts are installed separately.

```bash
git clone https://github.com/KiaroSama/Revayat-Subtitle-Skill.git
cd Revayat-Subtitle-Skill
python -m pip install -r skills/revayat-subtitle/requirements.txt
```

Then install into the agents you use:

```bash
# macOS / Linux
bash install/install.sh
```

```powershell
# Windows
./install/install.ps1
```

By default the installer selects existing user directories. Use `--agent codex`
for one agent, or `--scope project --path PATH` for a project. `--dry-run` lists
proposed destinations; `--force` keeps a backup before replacing an installation.
The Python implementation is shared by the Bash and PowerShell launchers.

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

Each destination receives a real `revayat-subtitle` directory. Use `--destination`
for a managed installation. Reload the agent after installation; Hermes project
skills also need its explicit trust flow. [Platform references](docs/platforms.md)
record the current discovery paths. The installer does not grant trust or install
prerequisites automatically.

### As a Claude Code plugin

```text
/plugin marketplace add KiaroSama/Revayat-Subtitle-Skill
/plugin install revayat-subtitle@revayat-subtitle-marketplace
```

The bundle includes `translate-subtitles`, `revayat-subtitle-resume` and
`revayat-subtitle-qa`; Claude exposes them under the `revayat-subtitle` namespace.

For a local portable plugin bundle:

```text
python install/install.py --plugin --destination "PATH/revayat-subtitle"
```

The root Agent Plugins manifest and Claude/Codex/Cursor compatibility manifests
share the same skill tree. Codex, Kiro Powers, Hermes and Antigravity use their
own plugin manager to import/enable a portable package. Cursor also accepts a
local copy under `~/.cursor/plugins/local/revayat-subtitle`. Cline and OpenCode use
native skill discovery here; their JS/TS extension systems are separate.

### Check the install

```bash
python skills/revayat-subtitle/scripts/revayat-subtitle.py doctor
```

`ready: true` confirms the FFmpeg filters are available. It does not prove that a
font can draw Persian or that a particular episode displays correctly. When
FFmpeg is outside PATH, use `--ffmpeg PATH` or `REVAYAT_FFMPEG`.
Actual image inspection is part of the workflow.

## Use

Tell the agent the anime, season and whether it is a new series or a sequel:

> Use revayat-subtitle for these subtitles. This is season 2 of the same anime.
> Keep the previous names and Japanese honorifics, write spoken Persian, preserve
> embedded fonts, and return Sub.zip plus the updated glossary.

Attach ASS/SRT files or a ZIP and the previous glossary when continuing a series.
In Codex, use `$revayat-subtitle`. With the Claude plugin, start with
`/revayat-subtitle:translate-subtitles`, resume with
`/revayat-subtitle:revayat-subtitle-resume`, or request a report with
`/revayat-subtitle:revayat-subtitle-qa`.

### Or drive it yourself

```bash
PY=python3                         # use a verified Python interpreter
S=skills/revayat-subtitle/scripts

"$PY" "$S/revayat-subtitle.py" doctor
"$PY" "$S/revayat-subtitle.py" prepare season.zip --work work/season01 --series "Anime title" --season 1
# Research the anime, settle glossary.json and map the episodes in project.json.
# Read every cue; complete every worksheet decision and its target text.
"$PY" "$S/revayat-subtitle.py" build --work work/season01
# Set BUILD to the build directory printed by the previous command.
"$PY" "$S/revayat-subtitle.py" render --build "$BUILD" --episode S01E01 --video episode01.mkv
# Open every PNG and record a real observation in its review file.
"$PY" "$S/revayat-subtitle.py" qa --build "$BUILD"
"$PY" "$S/revayat-subtitle.py" package --build "$BUILD" --out out/Sub.zip
```

In PowerShell set `$PY = 'python'` and `$S = 'skills/revayat-subtitle/scripts'`,
and invoke each command with `& $PY "$S/revayat-subtitle.py" ...`.
The complete file contracts are in [workflow.md](skills/revayat-subtitle/references/workflow.md).

**In:** ASS, SRT, directories and ZIP archives. **Out:** one subtitle per episode
inside `Sub/`, zipped, plus a separate continuity glossary. Files use `S01E01.ass`,
`S01OVA01.ass`, `S02E01.ass` and equivalent SRT names. SRT-only episodes stay SRT.
**Target:** Persian unless the user requests another language.

## How it works

```text
ASS / SRT / ZIP
       |
       v
prepare -> immutable sources + project.json + complete worksheets
       |                             |
       |                     research + locked glossary
       v                             |
compare releases <------------------+
       |
       v
the agent reads, translates and records every cue decision
       |
       v
build -> stable timing sort, structural cleanup, RTL, one file per episode
       |
       v
render -> FFmpeg PNGs -> the agent inspects and records observations
       |
       v
qa -> source, edition, glossary and image evidence must agree
       |
       v
package -> Sub.zip, read back and checked against the reviewed build
```

The agent owns meaning and visual judgment. Scripts own file structure and the
checks around those decisions. Sources are never rewritten; changing a worksheet
creates a different build identity and requires matching evidence.

| Module | Role |
| --- | --- |
| `subtitle_formats.py` | ASS/SRT document model, strict parsing, stable serialization, drawing/text separation and bidi marks |
| `markup.py` / `validation.py` | Shared override/direction parsing and strict editable-record contracts |
| `workflow.py` | source inventory, worksheets, glossary records, release merging and build identity |
| `render.py` | FFmpeg samples, shared QA gate and verified ZIP delivery |
| `runtime.py` | UTF-8 I/O, hashes, bounded subprocesses, staging directories and per-run logging |
| `process_control.py` / `process_supervisor.py` | Owned process groups/Windows Jobs with wall, idle and output limits |
| `publication.py` / `png_validation.py` | Complete-file publication and bounded PNG decoding |
| `revayat-subtitle.py` | one CLI entry point for every stage |
| `install/install.py` | shared agent routing and allowlisted skill/plugin installation |

## What it is honest about

- **A completed worksheet is a record, not proof of comprehension.** Semantic
  accuracy and visual inspection require the agent to do the work.
- **A neutral render is not a video-sync check.** Matching-video frames also show
  scene contrast, but do not establish audiovisual synchronization by themselves.
- **Styles are not interchangeable between scripts.** Complex donor markup,
  incompatible canvases, cuts and font sets require explicit reconciliation.
- **Player behavior varies.** Some difficult mixed-style lines need libass-specific
  adaptation; that is disclosed rather than presented as universal VSFilter support.
- **The real-media test was bounded.** Four full files totaling 5,188 events passed
  structural round trips; 40 sampled cues and ten actual-video images received
  editorial/visual review. That is not a full-season quality certificate.

See [verification scope](docs/verification.md). Private media is not distributed.
The selected real ASS files did not contain embedded font sections; font-payload
preservation/removal is covered mechanically by authored fixtures.

## Documentation

The skill routes to these references at the step that needs them:

- [SKILL.md](skills/revayat-subtitle/SKILL.md) — the nine steps and their completion criteria.
- [translation-policy.md](skills/revayat-subtitle/references/translation-policy.md) — spoken Persian, complete meaning and narrow removals.
- [persian-typography.md](skills/revayat-subtitle/references/persian-typography.md) — RTL, mixed scripts, quotes, fonts and rendered evidence.
- [source-languages.md](skills/revayat-subtitle/references/source-languages.md) — direct translation, Japanese, Chinese, French and Spanish profiles, and research for other languages.
- [research.md](skills/revayat-subtitle/references/research.md) — inspected repositories, adopted ideas and rejected defaults.
- [glossary-and-voice.md](skills/revayat-subtitle/references/glossary-and-voice.md) — research, names and sequel continuity.
- [release-selection.md](skills/revayat-subtitle/references/release-selection.md) — candidate comparison and episode identity.
- [subtitle-formats.md](skills/revayat-subtitle/references/subtitle-formats.md) — ASS/SRT, comments, styles, drawings and optional font removal.
- [workflow.md](skills/revayat-subtitle/references/workflow.md) — workspace, worksheet and review contracts.
- [parallel-editorial.md](skills/revayat-subtitle/references/parallel-editorial.md) — consent, assignments, worker logs and final integration.
- [troubleshooting.md](skills/revayat-subtitle/references/troubleshooting.md) — refusals, recovery and execution logs.

## Development

```bash
python -m pip install -r requirements-dev.txt
python tests/check.py
python tests/check.py --render
python evaluation/score.py --answers my-answers.json
```

The stdlib test runner bounds its worker process and exercises the real CLI and
native installers. Development dependencies add bounded generated cases and native
process-identity checks; the installed skill still has no pip runtime dependencies.
CI covers Linux, macOS and Windows, with libass and 10-bit preview lanes on Linux
and Windows. CodeQL analyzes Python and workflows; actionlint and zizmor block
workflow defects. Dependency Review runs on PRs; Dependabot covers Actions and
the development manifest. Read exact-commit Actions results before claiming a pass.

[evaluation/](evaluation/README.md) contains short authored language cases. Known
acceptable answers are recognized; unseen wording is reported for human review,
not declared a bad translation or given a fabricated quality score.

Runtime and installer logs are new UTF-8 UTC files for each run, under the skill's
`logs/` or `REVAYAT_LOG_DIR`. Separately, every agent using the skill must maintain
an activity log while researching, translating, correcting, merging and inspecting
the subtitles, and place that log beside the translated files as required by
`SKILL.md`. Chat updates and helper execution logs do not replace this agent log.

Execution logs record progress/errors without subtitle bodies;
`REVAYAT_LOG_LEVEL=DEBUG` enables bounded diagnostic detail (default `INFO`).
Read-only installations fall back to formatted stderr. Native installer bootstrap
logs also cover missing Python. See
[execution logs](skills/revayat-subtitle/references/troubleshooting.md#cli-execution-logs).

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

The same license accompanies standalone skill and plugin installations. FFmpeg,
fonts and the media being processed retain their own licenses and are not
redistributed by this repository.
