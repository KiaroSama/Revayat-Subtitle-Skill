# Verification scope

## Automated checks for v3.0.4

Run these from the repository root with Rust `1.100.0`, `rustfmt`, and `clippy`:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked --no-fail-fast
cargo build --workspace --release --locked
```

Install `ffmpeg` and `ffprobe` for media integration checks. Rendering needs an FFmpeg build with libass and the `subtitles` filter. Set `REVAYAT_REQUIRE_FFMPEG=1` to match CI's media test environment. After building, stage the local executables in `bin/` as shown in the [source build instructions](../README.md#build-from-source). Run `--help` and `--version` on both `revayat-subtitle` and `subsync`; run `revayat-subtitle doctor` to check the media setup.

The [CI workflow](../.github/workflows/ci.yml) runs on main pushes, pull requests, merge queue requests, and manual dispatch. Every supported 64-bit target builds and executes the workspace tests on its native host:

| Target | Both release binaries | Clippy and workspace tests with FFmpeg/libass |
| --- | --- | --- |
| Linux x64 | Build | Run |
| Linux arm64 | Build | Run |
| Windows x64 | Build | Run |
| Windows arm64 | Build | Run |

The Windows arm64 lane uses native ARM64 Rust binaries and the checksum-verified x64 FFmpeg through [Windows 11 application emulation](https://learn.microsoft.com/en-us/windows/arm/apps-on-arm-x86-emulation). It does not claim a native ARM64 FFmpeg build. Linux uses its host architecture's FFmpeg package. Media tests are required in all four lanes; an unavailable renderer is a failure.

Linux x64 also runs formatting. Separate jobs check workflows, tracked files, and plugin manifest versions. Use the [Actions results](https://github.com/KiaroSama/Revayat-Subtitle-Skill/actions) for the exact commit under review. Check `CI required` and the individual jobs; a listed job or a build on another architecture does not establish a passing result. The [release workflow](../.github/workflows/release.yml) depends on validation and packages both binaries for each supported target. See [release signature verification](signing/README.md) before using a downloaded binary.

The workspace suites check CLI contracts, subtitle parsing and preservation, Persian text mechanics, editorial state, installation recovery, process supervision, bounded rendering, sync, and no-clobber publication. PNG checks can show that a sample contains visible caption pixels and that a changed image invalidates its approval. They cannot judge what a cue means, whether a line reads naturally in Persian, or whether it sits in the right place over a particular shot.

## Verify a subtitle delivery

1. Import the source files, record their identities, and finish all six stages in the [translation workflow](../skills/revayat-subtitle/references/multi-agent-workflow.md). Check cue decisions, names, glossary, and episode mapping against the current sources.
2. Render the current build and inspect its images. Record observations against the build you inspected. If a correction changes that build, rerender and review the replacement. For a sync job, play the same dialogue track used for alignment and check cues near the beginning, middle, end, and each reported split. See the [FFmpeg sync guidance](../skills/revayat-subtitle/ffmpeg/SKILL.md).
3. Run `revayat-subtitle status --work WORK` and `revayat-subtitle qa --build BUILD`. Resolve pending or stale evidence before running `revayat-subtitle package --build BUILD --out out/Sub.zip` with a new output path. Read the archive back and confirm it contains one reviewed subtitle per episode under `Sub/`.

For a failed `revayat-subtitle` command, inspect its UTF-8 operational log as described in [Logging](../README.md#logging). Standalone `subsync` reports errors on stderr and can save its JSON result with `--json-report`; it does not create that operational logfile. Report the failing step and exit status. Check local paths before sharing a log; do not upload private input media, source subtitles, worksheets, or review images.

## Audit evidence

The [2026-11-28 edition migration recheck](audit-revayat-subtitle-2026-11-28.md) records an actual source-build upgrade that changed ASS output without changing recipe `19/16`. Generation `20` and normalization `17` give the corrected output a separate edition identity. The permanent migration target checks historical output bytes, unchanged reviewed inputs, preservation, idempotent rebuilds and refusal of missing or stale render evidence. Whenever generation or normalization changes emitted bytes for the same reviewed input, advance the applicable recipe versions in that change and verify migration from the preceding implementation; a green fresh-workspace test does not cover an existing edition.

The [2026-11-27 Rust 3.0.4 recheck](audit-rust-304-2026-11-27.md) records recovered-onset search bounds, clipped-background onset rejection, and format-aware FSI depth checks. Its new integration targets run in the existing four native CI lanes. Generation `19` and normalization `16` preserve earlier editions and require fresh render approval. The linked repair PR carries the final tested tree, head commit and hosted checks; earlier green checks do not validate a later commit.

The [2026-11-27 Rust 3.0.3 recheck](audit-rust-303-2026-11-27.md) records endpoint replay, bounded continuing-speech extension, sibling embedding preservation, and whole-cue bidi repair. Its permanent regressions run in the existing workspace CI; the linked repair PR supplies the exact final revision and hosted results. Generation `18` and normalization `15` require new render evidence without changing earlier editions.

The [2026-11-25 Rust audit](audit-rust-2026-11-25.md) records fresh v3 baseline tests, reproduced defects, implemented repairs, API/recipe migration, and the required review matrix. Its linked PR supplies the exact final revision and hosted CI receipt. Use that revision's results when evaluating the repairs.

### Earlier evidence

The [2026-11-24 reliability audit](audit-2026-11-24.md) records the earlier release's 14 defect groups, reproductions, and checks. It covers ASS preservation, line endings, sync staging, installer cleanup and recovery, and package destination collisions. Those results belong to the revisions named in the audit and do not establish v3.0.4 results.

On 2026-09-14, four real ASS candidates round-tripped 5,188 events without a structural difference. Reviewers read a 40-event excerpt and inspected render samples for its ten retained events over the original 10-bit video. They read back the delivery archive and confirmed the source subtitle hashes did not change. This was an excerpt check, not a review of all events or a full playback timing audit. Only authored fixtures are included in Git.

## Limits

- Each release needs results on its own commit. Host support does not mean the skill was run inside every agent UI.
- Mechanical render and timing checks do not establish translation quality or replace playback review. The earlier selected ASS files did not contain embedded fonts, so their renders do not establish compatibility with those payloads.
- Recovery and exclusive publication tests do not certify arbitrary power loss or network filesystem behavior. The sync subtitle and optional JSON report have separate final paths; a late report collision can leave the already complete subtitle in place.
