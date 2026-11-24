# Subtitle audio sync

Run the bridge from the repository root with Python 3.11 or newer. It looks for a release or debug binary under `rust/subsync-rs/target/`, then `subsync` on `PATH`. If neither exists, it uses Cargo to build the locked release from the checkout. An installed skill without Rust source needs `subsync` on `PATH`.

```text
python skills/revayat-subtitle/scripts/sync_bridge.py --video "episode.mkv" --in "original.ass" --out "aligned.ass" --json-report "report.json"
```

Video input needs FFmpeg. The binary asks FFmpeg for 16 kHz mono, signed 16-bit little-endian PCM and uses FFmpeg's automatic audio stream selection. To sync to a different track, supply its downmix as a mono 16-bit PCM WAV. Use `--wav` in place of `--video` for this and for offline tests. Choose fresh output paths in existing directories; the tool never overwrites source or destination files.

## How it estimates timing

- Speech activity: 10 ms hops and a 20 ms Hann window measure RMS relative to the background floor, zero crossings, and energy in the 300 to 3400 Hz band. The detector bridges gaps of at most 200 ms, extends speech by up to 100 ms, and filters bursts shorter than 60 ms.
- Global alignment: normalized FFT cross-correlation compares speech activity with rasterized cues. The engine tests the original speed and four ratios between 23.976 or 24 fps and 25 fps. It applies the ratio to subtitle timestamps and then adds the signed offset to move them toward the audio.
- Cuts: dynamic programming compares per-cue speech overlap against silence. It searches a local offset grid and correlation peaks and charges a penalty each time the offset changes. The report records the resulting stretches as `split_segments` with inclusive, one-based cue positions in time order. Those shifts describe the DP result before boundary clamping and snapping.
- Cue starts: the engine tries a nearby speech onset if it improves overlap, keeps at least 500 ms of cue duration, and does not collide with a neighbor. It skips snapping within intentional overlapping ASS groups.

The global search follows the [FFsubsync approach](https://github.com/smacke/ffsubsync#how-does-it-work). The split penalty follows the principle described by [ALASS](https://github.com/kaegi/alass). The engine implements both in Rust and does not run either external tool.

## Options

| Option | Default | Use |
|---|---:|---|
| `--ffmpeg PATH` | `ffmpeg` | Select an FFmpeg executable. No effect with `--wav`. |
| `--max-offset-sec N` | `60` | Search within N seconds of zero for the global offset, from 0 to 600. |
| `--max-split-offset-ms N` | `15000` | Search this far from the global offset for local shifts, from 0 to 60000. |
| `--split-penalty N` | `7.0` | Raise it to discourage cuts; lower it if a known edit is missed. |
| `--snap-ms N` | `350` | Limit changes to cue starts to this distance from the aligned position. Use `0` to disable. |
| `--no-fps-search` | off | Keep the original subtitle speed. |
| `--json-report PATH` | stdout only | Also save the JSON report to a new file. |
| `--timeout SECONDS` | `300` | Bound the engine subprocess. Python bridge only. |

## What stays intact

The parser accepts UTF-8 with or without an initial BOM and keeps the input's line endings. SRT timing uses millisecond precision; the engine retains cue numbers, multiline text, tags, and timing-line position settings. ASS/SSA timing uses centiseconds, with a minimum retimed span of 10 ms. The ASS reader preserves event order, comments, style fields, override tags, and unmodified sections such as `[Fonts]` and `[Graphics]`. ASS dialogue intervals that originally overlap can continue to overlap; the engine does not introduce overlap between independent groups. Cue-local effect and karaoke parameters are unchanged, so check their rendering after an FPS correction.

## Check the result

The report gives cue and speech segment counts, the FPS ratio, offset and split estimates, snapped cue count, and median and maximum start-time changes. `overlap_score_before` and `overlap_score_after` measure the fraction of cue time the detector marks as speech. They do not measure whether a line belongs to the person speaking.

If the report shows a split you do not expect, listen on both sides before adjusting `--split-penalty`. If the estimated FPS ratio is wrong, try `--no-fps-search`. If audio is silent, music masks most speech, or the selected audio track differs from the subtitle release, use the matching dialogue track or retime manually. Verify the beginning, middle, and ending by ear and revisit every reported split. Use playback to check signs, songs, and layers that have little or no spoken audio.
