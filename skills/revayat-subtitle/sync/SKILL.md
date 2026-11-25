---
name: revayat-subtitle-sync
description: Use when existing SRT or ASS subtitles lead, lag, drift, or fall out of sync after an edit and a matching video or WAV is available.
---

# Sync subtitles to audio

1. Confirm the subtitle and audio come from the same episode and language track. Choose a new output path with the same subtitle format. Keep the source file unchanged.
2. Run `revayat-subtitle sync` in process, add `--timeout 300` for a supervised whole-operation deadline, or use `subsync` directly. Video input automatically extracts the Front Center (`FC`) dialogue channel on 5.1/7.1 tracks, falling back to mono when that channel is unavailable or silent. PCM extraction preserves initial audio delay and timestamp gaps. If you need a different track, [probe and extract it](../ffmpeg/references/sync-and-probing.md) to a mono 16-bit PCM WAV and pass it with `--wav`.

   ```text
   revayat-subtitle sync --video "episode.mkv" --in "original.ass" --out "aligned.ass" --json-report "sync-report.json" --timeout 300
   subsync --wav "episode-dialogue.wav" --in "original.ass" --out "aligned-wav.ass" --json-report "wav-report.json"
   ```

   If music or effects dominate a 5.1/7.1 downmix, extract the Center (`FC`) dialogue channel with `pan=mono|c0=FC,aresample=async=1:first_pts=0` and pass the mono PCM WAV with `--wav`.

3. Per-dialogue cue-by-cue alignment is mandatory. Align every dialogue cue with its own spoken audio segment. Refine its start and end individually within neighbor-safe bounds, then snap eligible starts 50 ms before nearby speech onsets by default (`--lead-in-ms 50`). Keep back-to-back dialogue together without introducing gaps or dropping cues. Never rely solely on a uniform timeline shift. Check `fps_ratio`, `split_segments`, and speech overlap before and after. A higher score does not prove correct timing.
4. Play the result at the beginning, middle, end, and on either side of every split. Check that dialogue starts with the correct speaker, SRT cues stay in order, and ASS signs, effects, and layers still render correctly. Report unreviewed regions and keep the original if alignment is ambiguous.

See the [audio sync reference](../references/audio-sync.md) for alignment flags, file preservation, and cases that need manual timing. For FFmpeg stream selection and timing diagnosis, use the [FFmpeg skill](../ffmpeg/SKILL.md) and its [sync and probing reference](../ffmpeg/references/sync-and-probing.md).
