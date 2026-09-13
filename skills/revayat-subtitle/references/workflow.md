# Working files and commands

The runtime requires Python 3.10+ and FFmpeg with libass. Python uses only the
standard library. All text output is UTF-8; ASS uses logical Unicode, SRT numbering
is regenerated, and source timestamps stay unchanged unless explicitly reviewed.

## Import and resume

```text
python "SKILL_DIR/scripts/revayat-subtitle.py" doctor
python "SKILL_DIR/scripts/revayat-subtitle.py" prepare "season.zip" --work "work/season02" --series "Series title" --season 2 --glossary "previous/glossary.json"
```

Use `--target-language LANG` only for an explicitly requested non-Persian target.
`prepare` requires a new work directory. To resume, read its existing `project.json`,
`glossary.json`, and worksheets, then continue the pending decisions. Do not import
over them. Input limits: 1,000 subtitles, 16 MiB per subtitle, 256 MiB combined,
10,000 ZIP members. ZIP paths and symlinks are checked and no archive is extracted
into arbitrary paths. Encrypted/legacy archives need a separately authorized import.

`sources/` contains byte-identical copies, named by stable IDs. Worksheets contain
every Dialogue/SRT cue, including empty cues; hidden ASS Comment events are counted
in the inventory and removed without translation. Strict parsing fails on malformed
input instead of silently dropping unknown rows. Fix a separately preserved copy
and import again when a file needs a structural repair.

## Episode map

Edit only the configuration fields in `project.json`; preserve source identities,
paths, hashes and encodings. `episodes` starts empty to prevent guessed numbering.

```json
{
  "font_policy": "keep",
  "episodes": [
    {
      "id": "S02E01",
      "base": "s0001",
      "alternates": ["s0002"],
      "comparison": "ASS provides timed signs; the SRT resolves two dialogue mistranslations."
    },
    {
      "id": "S02OVA01",
      "base": "s0003",
      "alternates": [],
      "comparison": "Only candidate; complete dialogue and timing reviewed."
    }
  ]
}
```

This is an example of fields to edit, **not a replacement project file**. Every
source is assigned exactly once. Use `remove` instead of `keep` only for requested
embedded-font removal. Base ASS plus donor SRT remains ASS; SRT-only episodes remain
SRT. Ordinary SRT italic/bold/underline/strike markup is adapted for ASS donors.
More complex donor markup is refused for deliberate adaptation.

## Glossary

`glossary.json` holds `series`, `research`, `terms`, and `terms_reviewed`.
Research entries contain a real `url` and a `note` about the verified identity or
name. Terms contain `source`, `target`, `locked: true`, and optionally `aliases`,
`kind`, `source_url`, and `note`. Preserve these in following seasons.
Set `terms_reviewed` only after research and review. The builder checks that the
record is present; verifying the sources and applying names throughout the text
are the agent's responsibility. No fabricated URLs or meaningless signoffs.

## Every worksheet row

Keep `id` and `source_text` unchanged. Read all lines, fill `text` where needed,
choose an action, and set `reviewed: true` only afterwards.

| Action | Meaning |
| --- | --- |
| `edit` | Keep this cue using the complete reviewed `text`, including retained markup. |
| `preserve` | Keep the original nonlinguistic effect, symbol or already-correct literal. |
| `credit` | Remove verified promotion; a specific `note` is required. |
| `empty` | Remove a truly empty cue; prose and drawings prevent this action. |
| `alternate` | Omit duplicate candidate content; provide a `note` and `links` to retained cues. |

Links use `s0001:c000023`; a split/merged alternative can link to several retained
cues. To borrow a better translation while retaining ASS presentation, put the
improved wording in the base cue and mark the redundant donor `alternate`.
To retain content missing from the base, use `edit`/`preserve` on the donor cue.
Do not concatenate entire releases. Review timing and avoid duplicate retained speech.

`start_ms`/`end_ms` are integers; retiming requires `timing_note`. Changing protected
tags/drawings requires `structure_note` plus visual inspection. An explanation is
an audit record, not a correctness certificate. ASS physical line breaks use
literal `\N`; actual newline characters would corrupt its event format.

## Build, visual evidence and deliver

```text
python "SKILL_DIR/scripts/revayat-subtitle.py" build --work "work/season02"
python "SKILL_DIR/scripts/revayat-subtitle.py" render --build "BUILD" --episode S02E01 --all-cues
python "SKILL_DIR/scripts/revayat-subtitle.py" render --build "BUILD" --episode S02OVA01 --video "ova.mkv" --fonts-dir "fonts"
```

Build paths are derived from project, worksheet and glossary content. Keep the
build under its original workspace: render/package recheck that workspace. A changed
edit creates a new build; use that new path, not the old one. Repeated identical
builds are read back and checked, so accidental output edits are refused.

Each episode render writes a new PNG directory and `renders/EPISODE.json`.
Open each image; then set its `reviewed` to `true` and write a real observation in
`note`. Keep timestamps and hashes unchanged. Each FFmpeg child has a 45-second
bound; a timeout kills its owned process tree and blocks the command. A failed
render preserves earlier evidence and any diagnostic images; it never marks success.

```text
python "SKILL_DIR/scripts/revayat-subtitle.py" package --build "BUILD" --out "out/Sub.zip"
```

The ZIP must not already exist. It contains only `Sub/SxxEyy.ass` or `.srt`, and
OVA equivalents. Logs, source candidates, review JSON and glossary stay outside it.
Attach `BUILD/glossary.json` separately for continuity. `sync_verified: false` is
intentional: generated frames alone do not establish audiovisual synchronization.

## Logging and limits

Every CLI run writes `logs/revayat-subtitle_YYYY-MM-DD_HH-mm-ss_UTC.log` inside the
skill, with a suffix for collisions. Entries are UTC `[timestamp] [LEVEL]
 [COMPONENT] Message`. `REVAYAT_LOG_DIR` overrides the directory; a read-only
installation falls back to stderr. Logs include progress, error types and exit
status, not subtitle bodies. No verbose mode or automatic log deletion is enabled;
review logs for local path information before sharing and remove old logs as needed.

Semantic editing and image inspection need a capable agent with filesystem, shell,
web search and image-viewing access. Missing capabilities leave the relevant stage
unverified. Malformed/legacy formats, incompatible video cuts, complex cross-format
markup, different ASS canvases and conflicting embedded font sets require explicit
reconciliation; the helpers fail clearly instead of flattening or losing content.
