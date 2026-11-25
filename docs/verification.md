# Verification scope

## Automated checks for v3.0.0

Run these from the repository root with Rust `1.100.0`, `rustfmt`, and `clippy`:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
```

Install `ffmpeg` and `ffprobe` for media integration checks. Rendering needs an FFmpeg build with libass and the `subtitles` filter. Set `REVAYAT_REQUIRE_FFMPEG=1` to match CI's media test environment. After building, stage the local executables in `bin/` as shown in the [source build instructions](../README.md#build-from-source). Run `--help` and `--version` on both `revayat-subtitle` and `subsync`; run `revayat-subtitle doctor` to check the media setup.

The [CI workflow](../.github/workflows/ci.yml) runs on main pushes, pull requests, merge queue requests, and manual dispatch. Its 64-bit matrix separates builds from tests:

| Target | Both release binaries | Clippy and workspace tests with FFmpeg/libass |
| --- | --- | --- |
| Linux x64 | Build | Run |
| Linux arm64 | Build | Not run in this lane |
| Windows x64 | Build | Run |
| Windows arm64 | Build | Not run in this lane |

Linux x64 also runs formatting. Separate jobs check workflows, tracked files, and plugin manifest versions. Use the [Actions results](https://github.com/KiaroSama/Revayat-Subtitle-Skill/actions) for the exact commit under review. Check `CI required` and the individual jobs; a listed job or a build on another architecture does not establish a passing result. The [release workflow](../.github/workflows/release.yml) depends on validation and packages both binaries for each supported target. See [release signature verification](signing/README.md) before using a downloaded binary.

The workspace suites check CLI contracts, subtitle parsing and preservation, Persian text mechanics, editorial state, installation recovery, process supervision, bounded rendering, sync, and no-clobber publication. PNG checks can show that a sample contains visible caption pixels and that a changed image invalidates its approval. They cannot judge what a cue means, whether a line reads naturally in Persian, or whether it sits in the right place over a particular shot.

## Verify a subtitle delivery

1. Import the source files, record their identities, and finish all six stages in the [translation workflow](../skills/revayat-subtitle/references/multi-agent-workflow.md). Check cue decisions, names, glossary, and episode mapping against the current sources.
2. Render the current build and inspect its images. Record observations against the build you inspected. If a correction changes that build, rerender and review the replacement. For a sync job, play the same dialogue track used for alignment and check cues near the beginning, middle, end, and each reported split. See the [FFmpeg sync guidance](../skills/revayat-subtitle/ffmpeg/SKILL.md).
3. Run `revayat-subtitle status --work WORK` and `revayat-subtitle qa --build BUILD`. Resolve pending or stale evidence before running `revayat-subtitle package --build BUILD --out out/Sub.zip` with a new output path. Read the archive back and confirm it contains one reviewed subtitle per episode under `Sub/`.

For a failed command, inspect the UTF-8 log in `logs/<binary>_YYYY-MM-DD_HH-mm-ss_UTC.log`. Report the failing step and exit status. Check local paths before sharing a log; do not upload private input media, source subtitles, worksheets, or review images.

## Earlier evidence

The [2026-11-24 reliability audit](audit-2026-11-24.md) records the earlier release's 14 defect groups, reproductions, and checks. It covers ASS preservation, line endings, sync staging, installer cleanup and recovery, and package destination collisions. Those results belong to the revisions named in the audit and do not establish v3.0.0 results.

On 2026-09-14, four real ASS candidates round-tripped 5,188 events without a structural difference. Reviewers read a 40-event excerpt and inspected render samples for its ten retained events over the original 10-bit video. They read back the delivery archive and confirmed the source subtitle hashes did not change. This was an excerpt check, not a review of all events or a full playback timing audit. Only authored fixtures are included in Git.

## Limits

- Each release needs results on its own commit. Host support does not mean the skill was run inside every agent UI.
- Mechanical render and timing checks do not establish translation quality or replace playback review. The earlier selected ASS files did not contain embedded fonts, so their renders do not establish compatibility with those payloads.
- Recovery and exclusive publication tests do not certify arbitrary power loss or network filesystem behavior. The sync subtitle and optional JSON report have separate final paths; a late report collision can leave the already complete subtitle in place.
