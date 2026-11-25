---
description: Align an SRT or ASS subtitle to a matching video or mono PCM WAV.
argument-hint: --video <video> --in <subtitle> --out <new subtitle>
---

Use the [audio sync reference](../skills/revayat-subtitle/references/audio-sync.md)
with `$ARGUMENTS`. Accept exactly one video or WAV input, a source subtitle and a
new output path. Ask for missing paths. Pass each path as one argument.

```text
revayat-subtitle sync --video "episode.mkv" --in "original.ass" --out "aligned.ass" --json-report "sync-report.json"
```

Use `--wav` in place of `--video` for mono 16-bit PCM WAV input. The standalone
`subsync` command accepts the same paths. Report offset, FPS ratio, splits and
change in speech overlap. State separately what you checked in playback.
