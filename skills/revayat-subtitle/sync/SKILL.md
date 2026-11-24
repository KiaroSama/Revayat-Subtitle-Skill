---
name: revayat-subtitle-sync
description: Use when existing SRT or ASS subtitles lead, lag, drift, or fall out of sync after an edit and a matching video or WAV is available.
---

# Sync subtitles to audio

1. Confirm the subtitle and audio come from the same episode and language track. Choose a new output path with the same subtitle format. Keep the source file unchanged.
2. Run the bridge from the project root. It runs the Rust `subsync` binary, which uses FFmpeg to extract 16 kHz mono PCM when given a video. For a local mono 16-bit PCM WAV, replace `--video` with `--wav`.

   ```text
   python skills/revayat-subtitle/scripts/sync_bridge.py --video "episode.mkv" --in "original.ass" --out "aligned.ass" --json-report "sync-report.json"
   ```

3. Read the JSON report. The Rust engine uses FFT correlation for global offset and FPS drift, split-penalty dynamic programming for mid-episode edits, and bounded speech-onset snapping for eligible cues. Check `fps_ratio`, `split_segments`, and the before/after speech overlap. A higher score is not proof of correct timing.
4. Play the result at the beginning, middle, end, and on either side of every split. Check that dialogue starts with the correct speaker, SRT cues stay in order, and ASS signs, effects, and layers still render correctly. Report unreviewed regions and keep the original if alignment is ambiguous.

See [audio sync reference](../references/audio-sync.md) for flags, file preservation, and cases that need manual timing.
