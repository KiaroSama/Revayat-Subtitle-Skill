# Revayat Subtitle

[فارسی](README.fa.md)

Translate English anime subtitles into natural spoken Persian, or edit Persian
subtitles completely. A companion to
[Revayat Comic](https://github.com/KiaroSama/Revayat-Comic-Skill) and
[Revayat Novel](https://github.com/KiaroSama/Revayat-Novel-Skill), built around the
same division of work: **the agent reads and translates; scripts handle files and verification**.

- Read every cue in every candidate, with researched names and a reusable series glossary.
- Preserve Japanese honorifics, story content, profanity, signs, drawings and effects.
- Prefer ASS presentation, reconcile better candidate content, sort timings and repair RTL punctuation.
- Remove verified advertising, empty cues, hidden comments and unused styles; keep embedded fonts by default.
- Inspect FFmpeg renders and deliver one `S01E01.ass` / `S01OVA01.ass` per episode inside `Sub.zip`.

SRT-only inputs remain SRT. Embedded-font removal is explicitly optional. No paid
translation API, Python package dependency, OCR engine, or background service is required.

## Install the skill

Requires Python 3.10+ and FFmpeg built with libass for visual QA. Use an agent with
file editing, shell execution, web search and image viewing. Download or clone
[this repository](https://github.com/KiaroSama/Revayat-Subtitle-Skill).

Windows (PowerShell):

```powershell
./install/install.ps1 --agent codex
./install/install.ps1 --agent claude --scope project --path 'C:\Projects\Anime'
```

Linux/macOS (Bash):

```bash
bash install/install.sh --agent codex
bash install/install.sh --agent claude --scope project --path "$PWD"
```

Portable alternative: `python install/install.py --agent codex`. Use `--agent all`
to install to detected user directories, `--dry-run` to list destinations,
`--destination PATH` for an exact directory, or `--force` to retain a backup before
replacing an existing installation. Installers do not install Python or FFmpeg,
edit global instruction files, grant trust, or enable extensions.

| Agent | User skill directory | Project skill directory |
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

The `revayat-subtitle` folder is appended to these directories. Restart/reload your
agent after installation. Hermes project skills require its explicit trust flow.
Use `$revayat-subtitle` in Codex or ask the agent to use the skill by name.

## Install as a plugin

The repository includes a portable root `plugin.json` for Agent Plugins, plus
Claude, Codex and Cursor compatibility manifests. All share `skills/revayat-subtitle`.
Claude also exposes `/revayat-subtitle:translate-subtitles`.

Claude marketplace installation:

```text
claude plugin marketplace add KiaroSama/Revayat-Subtitle-Skill
claude plugin install revayat-subtitle@revayat-subtitle-marketplace
```

For a local plugin copy, use the same installer with an explicit destination:

```text
python install/install.py --plugin --destination "PATH/revayat-subtitle"
```

For Cursor that destination can be `~/.cursor/plugins/local/revayat-subtitle`
(expand `~` in the calling shell); reload and inspect Customize. Codex, Kiro Powers,
Hermes and Antigravity can consume portable plugin packages through their own
plugin manager. Installation and enablement may be separate; follow the host's
instructions. Cline/OpenCode use their native **skill** discovery for this bundle;
their JS/TS extension systems are different products.

See [current platform references](docs/platforms.md) for documentation and limits.
Manifest compatibility is separate from testing inside a particular installed app.

## Use it

Tell the agent the anime title, season, whether this is a new anime or a sequel,
and attach your ASS/SRT files or ZIP. Supply the previous glossary for continuity.

> Use revayat-subtitle. This is season 2 of the same anime. Review all candidates,
> keep the previous names and Japanese honorifics, use spoken Persian, preserve
> embedded fonts, and return Sub.zip plus the updated glossary.

The full workflow is in [SKILL.md](skills/revayat-subtitle/SKILL.md). The CLI stages are
`doctor`, `prepare`, `build`, `render`, `package`; see the
[file contracts and examples](skills/revayat-subtitle/references/workflow.md).

```text
python skills/revayat-subtitle/scripts/revayat-subtitle.py doctor
```

If FFmpeg is outside PATH, set `REVAYAT_FFMPEG` or pass `--ffmpeg PATH` to doctor
and render. The renderer uses actual FFmpeg/libass, with complex shaping for ASS.
The [FFmpeg filter documentation](https://ffmpeg.org/ffmpeg-filters.html#ass)
describes the required build support. A Persian-capable font must also be present.

## Verification and limitations

Every candidate cue needs a recorded review. The build verifies IDs, source hashes,
episode assignments, references, times and structural edits. The final ZIP gate
checks render coverage, explicit visual observations, image hashes and subtitle
bytes, then reads back the ZIP. These gates cannot prove semantic quality or that
an agent really inspected an image; those remain explicit responsibilities.

Without the actual episode video, renders use a neutral background and do not
prove audiovisual synchronization or scene contrast. ASS/SRT are supported;
SSA/VTT/PGS, encrypted archives and malformed files need deliberate conversion.
Different ASS canvases, complex donor markup and conflicting embedded font sets
are refused until reconciled. Font availability and bidi behavior vary by player.
No real anime attachment was supplied when the skill was created; the repository
fixtures cover structural and visual mechanics, not production translation quality.

Run the bounded check with `python tests/check.py`; add `--render` when FFmpeg is
available. The included CI matrix checks Windows, Linux and macOS and runs libass
rendering on Linux. Application-specific discovery/enablement needs testing in the
actual installed host.

Every executable run writes a new UTC log under the skill's `logs/`, or
`REVAYAT_LOG_DIR`. Read-only installations fall back to stderr. Names follow
`revayat-subtitle_YYYY-MM-DD_HH-mm-ss_UTC.log` (installer: `install_...`), with
collision suffixes. Entries contain levels, progress, errors and exit status,
never subtitle bodies. Logs are retained until the operator removes them.
