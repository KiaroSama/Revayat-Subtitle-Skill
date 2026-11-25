---
name: ffmpeg
description: Use when subtitle timing depends on choosing the right audio track, inspecting FFmpeg streams or timestamps, or diagnosing offset, drift, or render errors.
---

# FFmpeg for subtitle sync

Revayat Subtitle v3.0.0 uses FFmpeg to read media and the Rust CLI to align SRT or ASS cues. Use local `ffmpeg` and `ffprobe` for inspection. No helper scripts are needed. A build with libass is needed only for the optional subtitle render check.

1. Probe the matching release. Confirm the picture, dialogue track, stream start times, and duration. On a multi-track release, check the language and title tags, then listen if the tags are missing or ambiguous.
2. Use video directly if FFmpeg's automatic audio choice is the track you want. To control which audio track is used, [extract it as a 16 kHz mono PCM WAV](references/sync-and-probing.md#extract-the-chosen-track), verify it, and pass it with `--wav` instead of `--video`.
3. Run one of the commands below. Keep the source files unchanged and choose new paths for the aligned subtitle and report. Keep the subtitle format unchanged.

   ```text
   revayat-subtitle sync --video "Episode 01.mkv" --in "original.ass" --out "aligned.ass" --json-report "sync-report.json"
   subsync --wav "Episode 01 dialogue.wav" --in "original.ass" --out "aligned-wav.ass" --json-report "sync-wav-report.json"
   ```

4. Compare the report with actual playback using the same dialogue track. Watch cues near the start, middle, end, and on both sides of each reported split. If timing remains ambiguous, keep the original and say which parts still need review.

See [sync and probing](references/sync-and-probing.md) for stream mapping, timestamp checks, and Windows and Linux quoting. See the [sync skill](../sync/SKILL.md) for alignment and review.

Adapted from [FFmpeg Skill v4.7.1](https://github.com/kajisho5/ffmpeg-skill) (MIT). The [upstream license](LICENSE) is included.
