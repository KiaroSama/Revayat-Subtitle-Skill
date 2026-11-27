<div align="center">

# Revayat Subtitle

[![GitHub Actions CI](https://img.shields.io/github/actions/workflow/status/KiaroSama/Revayat-Subtitle-Skill/ci.yml?style=flat-square&label=CI&color=1d4ed8)](.github/workflows/ci.yml)
[![Repository license](https://img.shields.io/github/license/KiaroSama/Revayat-Subtitle-Skill?style=flat-square&color=2563eb)](LICENSE)
[![Latest published release](https://img.shields.io/github/v/release/KiaroSama/Revayat-Subtitle-Skill?style=flat-square&color=ea580c)](https://github.com/KiaroSama/Revayat-Subtitle-Skill/releases)
[![Latest Git tag](https://img.shields.io/github/v/tag/KiaroSama/Revayat-Subtitle-Skill?style=flat-square&label=Tag&color=c2410c)](#releases-and-tags)
[![Rust workspace tests](https://img.shields.io/badge/Tests-Rust%20Workspace-0d9488?style=flat-square)](#verification)
[![Pinned Rust version](https://img.shields.io/badge/Rust-1.100.0-8b5cf6?style=flat-square)](rust-toolchain.toml)
[![64-bit Windows and Linux](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20(64--bit)-0891b2?style=flat-square)](docs/platforms.md#supported-platforms)
[![Install with the Rust CLI](https://img.shields.io/badge/Install-Rust%20CLI-4f46e5?style=flat-square)](#install)
[![English and Persian documentation](https://img.shields.io/badge/Docs-English%20%7C%20Persian-0284c7?style=flat-square)](#documentation)
[![GitHub repository size](https://img.shields.io/github/repo-size/KiaroSama/Revayat-Subtitle-Skill?style=flat-square&color=ca8a04)](#repository-layout)
[![FFmpeg and libass for rendering](https://img.shields.io/badge/Render-FFmpeg%20%7C%20libass-334155?style=flat-square)](#media-tools)
[![ASS with bundled B Koodak](https://img.shields.io/badge/Default%20output-ASS%20%7C%20B%20Koodak-15803d?style=flat-square)](#subtitle-formats)
[![Two standalone binaries](https://img.shields.io/badge/Binaries-2-7c3aed?style=flat-square)](#command-reference)
[![Six translation stages](https://img.shields.io/badge/Translation-6%20stages-b45309?style=flat-square)](#translation-workflow)
[![Support donations](https://img.shields.io/badge/Support-donations-d04a9a)](#donate)

</div>

[فارسی](README.fa.md)

Revayat Subtitle is a skill for translating and reviewing subtitles, with tools to convert ASS and SRT, check rendered frames, and align cues to dialogue audio. Persian is the default translation language. The final ZIP contains the reviewed episode subtitles; the series glossary is delivered separately.

Version `3.0.4` ships two standalone binaries written entirely in Rust: `revayat-subtitle` and `subsync`. The project uses Rust `1.100.0`, edition `2024`, and ships no Python, PowerShell, or Bash scripts. Releases and CI target 64-bit Windows and Linux only, on x64 and arm64.

## Install

Download both binaries for your OS and architecture from [GitHub Releases](https://github.com/KiaroSama/Revayat-Subtitle-Skill/releases) and [verify their signatures](docs/signing/README.md) before running them. Release filenames include the tag and target. Keep both binaries together under the local names shown below and put their directory on the agent's `PATH`.

The binaries run directly. To install the agent skill, get a repository checkout for the same release tag. From its root, run `revayat-subtitle install --dry-run`, then `revayat-subtitle install` after reviewing the destinations. The installer reads the skill, command, and manifest files from that checkout.

### Build from source

From the repository root, with Rust `1.100.0` and the native target's linker available, build both binaries and copy Cargo's outputs into the project's `bin/` directory.

Windows (PowerShell 7), producing `bin/revayat-subtitle.exe` and `bin/subsync.exe`:

```powershell
cargo build --workspace --release --locked
if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
New-Item -ItemType Directory -Force -Path bin | Out-Null
Copy-Item -LiteralPath target/release/revayat-subtitle.exe, target/release/subsync.exe -Destination bin/
./bin/revayat-subtitle.exe install --dry-run
```

Linux (POSIX shell), producing `bin/revayat-subtitle` and `bin/subsync`:

```sh
cargo build --workspace --release --locked &&
  mkdir -p bin &&
  cp target/release/revayat-subtitle target/release/subsync bin/ &&
  ./bin/revayat-subtitle install --dry-run
```

After checking the destinations, repeat the `install` command without `--dry-run`. The default install selects existing user-level agent roots. Use `revayat-subtitle install --agent codex` to select one host, or `revayat-subtitle install --agent codex --scope project --path .` to install into the current project. If you pass `--force`, the installer backs up the existing installation before replacing it. Keep `bin/` on the host's `PATH` and reload the agent. See [host paths and recovery](docs/platforms.md#agent-installation).

Before reverting binaries, retain a compatible installer for existing journals and backups; see [installer downgrade compatibility](docs/platforms.md#installer-downgrade-compatibility).

### Plugin installation

The same skill tree ships in five manifest formats: Claude Code, Cursor, Codex, Antigravity, and Agent Plugins 1.0. In Claude Code:

```text
/plugin marketplace add KiaroSama/Revayat-Subtitle-Skill
/plugin install revayat-subtitle@revayat-subtitle-marketplace
```

The plugin exposes `translate-subtitles`, `revayat-subtitle-resume`, `revayat-subtitle-qa`, and `revayat-subtitle-sync` under the `revayat-subtitle` namespace. Keep the CLI binaries available to the host. See [manifest paths](docs/platforms.md#plugin-manifests) for the other formats.

## Media tools

For visual rendering and audio extraction, put `ffmpeg` and `ffprobe` on `PATH`. Rendering needs FFmpeg built with `libass` and the `subtitles` filter. Run `revayat-subtitle doctor` to check prerequisites; `ready: true` does not replace checking the actual rendered subtitles.

`revayat-subtitle sync` and `subsync` align each dialogue cue individually with the spoken audio. Back-to-back dialogue stays together, and eligible cues start 50 ms before speech by default (`--lead-in-ms 50`). End refinement defaults to `--end-adjust-ms 600` and targets about 250 ms after speech, with about 900 ms for reading when the gap allows. Video sync automatically selects the Front Center (`FC`) dialogue channel on 5.1/7.1 tracks and falls back to a mono downmix when needed.

Use the [imported FFmpeg Skill guidance](skills/revayat-subtitle/ffmpeg/SKILL.md), adapted from `ffmpeg-skill` v4.7.1, to identify the matching release and dialogue track. If a video has several audio tracks, extract the selected track as 16 kHz mono PCM WAV and pass it with `--wav`. Keep the original subtitle and check results in playback near the beginning, middle, end, and any reported split.

```text
revayat-subtitle sync --video "Episode 01.mkv" --in "original.ass" --out "aligned.ass" --json-report "sync-report.json"
subsync --wav "Episode 01 dialogue.wav" --in "original.ass" --out "aligned-wav.ass" --json-report "sync-wav-report.json"
```

## Subtitle formats

The default Persian ASS style uses `B Koodak` at size `110` on a 1920×1080 canvas, with outline and shadow `2` and left, right, and vertical margins of `10`. New styles are non-bold unless the source ASS Default style is bold or you set `--bold`; use `--no-bold` to override a bold source. Persian display lines use `RLM` + `RLI` ... `PDI`. Conversion clears unbalanced bidi controls, repairs intra-word `RLM` to ZWNJ, and moves reversed leading punctuation to the logical end. The converter preserves supported ASS styling and effects, normalizes Persian letters and ZWNJ, and wraps at phrase boundaries without splitting tags. It logs unmistakable credit removals and keeps ambiguous dialogue. Use `--keep-watermarks` to retain credits.

```text
revayat-subtitle convert "episode.srt" --out "episode.ass"
revayat-subtitle translator-ass "base.ass" --donor "donor.srt" --source-comments --out "worksheet.ass"
```

Translator Mode keeps the primary release's overlap groups, fills uncovered timing slots from a donor release, and can store source text as non-display comments. For SRT delivery, choose `--output-format srt` at import so review and build use the same format. See [format and typography details](skills/revayat-subtitle/references/subtitle-formats.md) and the [font notice](skills/revayat-subtitle/assets/fonts/NOTICE.txt).

## Translation workflow

The coordinator prepares a workspace from ASS, SRT, a folder, or a ZIP, checks the episode map, and locks the glossary before handing off the work. Original source files and cue IDs stay intact.

1. Initial Translation: Draft each cue in context and flag unresolved lines.
2. Typesetting & Layout: Preserve placement and effects; check line breaks, font coverage, and Persian right-to-left punctuation.
3. Editorial Review: An independent reviewer checks fidelity, dialogue, names, and continuity, then records decisions against the exact worksheet revision.
4. Applying Translation: Apply the accepted decisions, build the edition, and align timing if requested.
5. File Re-Check & Issue Repair: Fix issues, rebuild, and inspect fresh render samples from the current edition.
6. Final Review & Delivery: An independent reviewer checks the current build and its evidence, then packages one subtitle per episode under `Sub/` in `Sub.zip`.

The bundled [humanizer v4.0.0](skills/revayat-subtitle/voice/humanizer/SKILL.md) helps editors remove formulaic phrasing without changing meaning. The [Persian guide](skills/revayat-subtitle/voice/persian/SKILL.md) covers spoken register, نیم‌فاصله (ZWNJ), Persian letters, and punctuation. [Target voice guides](skills/revayat-subtitle/references/target-voice.md) also cover English and Russian.

Start a workspace with:

```text
revayat-subtitle prepare "season.zip" --work "work/season01" --series "Anime title" --season 1
revayat-subtitle status --work "work/season01"
```

Run the build after the editorial decisions are accepted. Keep the build in its original workspace and substitute its reported path for `BUILD`:

```text
revayat-subtitle build --work "work/season01"
revayat-subtitle render --build "BUILD" --episode S01E01 --video "episode01.mkv"
```

Inspect the PNGs and record what you saw. After QA confirms the current build and its review, package to a fresh ZIP path:

```text
revayat-subtitle qa --build "BUILD"
revayat-subtitle package --build "BUILD" --out "out/Sub.zip"
```

An unrun stage stays pending. If an edit changes the build, rerender it and renew its visual approvals. The [six-stage procedure](skills/revayat-subtitle/references/multi-agent-workflow.md) specifies the handoffs, including how to resume without repeating accepted work.

## Command reference

`revayat-subtitle` has 16 subcommands. Run `revayat-subtitle <subcommand> --help` for its options. Both `revayat-subtitle` and `subsync` accept `--help` and `--version`.

| Subcommand | Use |
| --- | --- |
| `doctor` | Check the media tools and libass support. |
| `prepare` | Import ASS, SRT, folders, or ZIP files and create review worksheets. |
| `build` | Validate cue decisions and write the reviewed edition. |
| `convert` | Convert SRT or ASS to Persian-ready ASS. |
| `translator-ass` | Run `convert --translator-mode` to make an ASS worksheet with donor slots. |
| `sync` | Align ASS or SRT to matching video or WAV audio. |
| `render` | Render visual samples with FFmpeg and validate the PNGs. |
| `qa` | Run read-only delivery checks against a reviewed build. |
| `package` | Write a delivery ZIP without replacing an existing file. |
| `status` | Report pending work and current build evidence without editing the workspace. |
| `handoff` | Validate revision-bound editorial proposals without applying them. |
| `supervise` | Run a child process under bounded process-tree supervision. |
| `install` | Install the skill or recover an interrupted installation. |
| `lint` | Check or fix Persian text mechanics and style. |
| `lookup` | Search the bundled glossary and idiom tables. |
| `score` | Score a submission against the review rubric. |

`subsync` is the standalone audio alignment CLI for SRT and ASS. It can write a JSON timing report. For `status` and `handoff`, exit `0` means complete, `1` means valid but pending, and `2` means invalid or stale. See [status and handoff contracts](skills/revayat-subtitle/references/workflow.md#read-only-status-and-editorial-handoffs).

## Logging

`revayat-subtitle` execution uses UTF-8 logs named `logs/revayat-subtitle_YYYY-MM-DD_HH-mm-ss_UTC.log`, with a suffix when a filename would collide. Each line uses `[YYYY-MM-DD HH:mm:ss UTC] [LEVEL] [COMPONENT] Message`. Logs record progress, failures, and exit status without subtitle bodies or secrets. The levels are `DEBUG`, `INFO`, `WARNING`, `ERROR`, and `CRITICAL`; use `--log-level` for more detail. Logs are not deleted automatically. Review local path information before sharing one with a bug report. Agent activity logs are separate from these CLI logs. The standalone `subsync` binary writes its JSON result to stdout and diagnostics to stderr; use `--json-report` to save its result, or run through `revayat-subtitle sync` for operational file logging.

## Verification

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

CI builds both binaries for all four targets and runs workspace tests with FFmpeg/libass on Windows x64 and Linux x64. Check the [verification scope](docs/verification.md) for which checks run where. Rust checks and PNG tests cannot establish translation accuracy or confirm timing in your player; those require cue review and playback on the matching media.

## Documentation

- [Main skill](skills/revayat-subtitle/SKILL.md) and [working files and commands](skills/revayat-subtitle/references/workflow.md)
- [Audio sync reference](skills/revayat-subtitle/references/audio-sync.md) and [Persian typography](skills/revayat-subtitle/references/persian-typography.md)
- [Source language rules](skills/revayat-subtitle/references/source-languages.md) and [evaluation rubric](evaluation/rubric.md)
- [Persian README](README.fa.md) and [changelog](CHANGELOG.md)

## Repository layout

- `crates/`: Rust workspace code for the CLI, subtitle processing, and audio alignment.
- `skills/` and `commands/`: Agent instructions, workflow references, voice guides, and bundled assets.
- `docs/`: Platform, verification, and signing references.
- `bin/`: Local release binaries; generated and not tracked.

<a id="releases-and-tags"></a>

## Releases and tags

The badges update automatically from [GitHub releases](https://github.com/KiaroSama/Revayat-Subtitle-Skill/releases) and [Git tags](https://github.com/KiaroSama/Revayat-Subtitle-Skill/tags). A tag can appear before its release files are published.

## Author

Kiaro Sama ([KiaroSama](https://github.com/KiaroSama)) · [Kiaro.Sama.Dev@gmail.com](mailto:Kiaro.Sama.Dev@gmail.com)

## License

Copyright © 2026 Kiaro Sama. Released under the [GNU General Public License v3.0 or later](LICENSE).

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
