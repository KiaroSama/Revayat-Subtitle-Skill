# Revayat Subtitle: edition migration recheck

Review date: **2026-11-28, Asia/Tehran**.

## Revision and bounded scope

This recheck starts at main commit `521f23120f95df3357571bb249dcb7830e004116`,
tree `7962588056f53f03617b432d13379215c8978534`. The owner had merged
[PR #42](https://github.com/KiaroSama/Revayat-Subtitle-Skill/pull/42) and closed
[issue #41](https://github.com/KiaroSama/Revayat-Subtitle-Skill/issues/41).
Those repairs remain regression controls. The subsequent main commit changed
visible ASS direction detection, mode-2 conversion breaks, Gitleaks concurrency
and ignored Cargo directories. This audit accepts one new reproducible defect,
R28-01, rather than reopening earlier findings without new evidence.

The untouched baseline passed the release build, formatting, Clippy with warnings
denied, and the entire workspace with `REVAYAT_REQUIRE_FFMPEG=1` and
`--no-fail-fast`: **1276 passed, 0 failed, 0 ignored**, across 64 summary rows.
The local test build disabled debug information to fit the audit environment;
that does not stand in for native hosted results. Actual baseline
[CI run 46069949544](https://github.com/KiaroSama/Revayat-Subtitle-Skill/actions/runs/46069949544)
also completed release builds, Clippy and FFmpeg/libass tests on all four hosts:

| Native Rust target | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| Linux x64 | 1276 | 0 | 0 |
| Linux ARM64 | 1276 | 0 | 0 |
| Windows x64 | 1244 | 0 | 0 |
| Windows ARM64 | 1244 | 0 | 0 |

All 11 checks on that exact baseline SHA succeeded. Windows ARM64 uses native
Rust executables and the workflow's verified x64 FFmpeg under Windows emulation.
Final repair results belong to the repair PR's exact head; the baseline numbers
above do not certify a later commit.

## R28-01: changed normalization reuses an existing edition identity

**Impact:** a source-built upgrade can refuse to rebuild an unchanged, already
reviewed workspace. QA instructs the user to rebuild, but that rebuild collides
with the previous edition and fails. The integrity checks prevent overwriting;
the defect is failure to identify the changed generation recipe.

The old implementation at `fbebb82ca0a61ff039681ae8ea15d476e342031e` and the
baseline main implementation both declare generation `19`, normalization `16`.
The latter correctly stops treating the letters in ASS `\h` and soft `\n` as
visible Latin text. For a Persian-only cue explicitly reviewed with direction
`ltr`, the old output contains `LRE + LRM + text + LRM + PDF`, while the corrected
output contains `LRM + text + LRM`. The raw ASS escape remains unchanged.

`workflow::workspace::assemble` derives the edition identity from the reviewed
input identity, generation recipe and output format. Output hashes are recorded
in the manifest but are not inputs to that edition identity. Changing output
while leaving the recipe unchanged therefore gives different manifests the same
`builds/<identity>` destination. `workspace::build` correctly rejects that
collision with:

```text
ValueError: Existing build has been modified; restore it or create a fresh workspace
```

### Actual binary reproduction

This was reproduced with two independently frozen optimized executables, not
only by editing a synthetic manifest:

| Source implementation | `revayat-subtitle` SHA-256 |
| --- | --- |
| Previous PR #42 head, `fbebb82` | `5cf2316ebc8da60174d909f5b82e025055dd71fc98283035cb3640a026d27e39` |
| Baseline main, `521f231` | `e853b5061d39829e93bf4532c099554df18aba937b794c20360144677a5b520f` |

1. Prepare an authored single-cue ASS file with `WrapStyle: 0` and text
   `سلام\hدنیا` or `سلام\nدنیا`. Record valid project, glossary and worksheet
   decisions, with an explicit `ltr` direction and note.
2. Build using the older executable. Preserve the complete workspace, its input
   identity, edition identity, manifest and output bytes.
3. Build that same workspace using the baseline executable. Both affected cases
   exit `2` with the collision diagnostic. No preexisting file changes.
4. Build a copy of the exact reviewed inputs that has no previous edition. It
   succeeds with the same input identity, same `19/16` recipe and same edition
   identity, but a different subtitle SHA-256. This isolates the missing recipe
   change from tampering, import changes or invalid reviews.
5. Repeat with literal-space and genuine Latin-text controls. Their output bytes
   do not change and existing builds are reused successfully.

For the hard-space case the old/new subtitle hashes are respectively
`b45f8f2f8757e03d07cbf9285c4b1bddbd6b517dad49835d8aa87414f8aada8a`
and `6fe6c17b6a3d598df3d6997efd1fdbd4ec37592a0f531d0bb9fe858bfc6fbdf0`.
Both implementations use edition identity
`31417bf118f27995cd68d7d83bf439d6c9f31dcc73d6b4b02f0df5129c16d07a`.

The actual older executable also rendered a nonblank libass image. That image
was opened and its authored caption inspected before recording an observation;
old QA and packaging then succeeded. Baseline build, QA and packaging all refused
the unchanged old workspace. Ten protected files, including its genuine render
receipt and image, retained their bytes, and refused packaging created no ZIP.
No translation-quality claim is inferred from this technical fixture.

### Implemented repair

Advance `RECIPE_VERSION` in `crates/revayat-core/src/workflow/workspace.rs` to
generation `20` and normalization `17`. Keep the reviewed input identity,
manifest schema, centisecond policy and render sampler unchanged. The corrected
output now has a distinct edition directory. Old editions remain intact and
fresh render evidence is required for the replacement.

Do not weaken manifest or byte comparisons, overwrite the old edition, delete
its directory, silently inherit visual approval, or ask the user to recreate the
workspace. Those approaches conceal the collision or discard valid history.
All render, delivery and status consumers derive the current manifest through
the shared assembly path; no duplicate runtime recipe counter needs alteration.

### Permanent regressions and closure

`crates/revayat-core/tests/ass_recipe_migration.rs` adds three integration tests:
hard-space migration, soft-space migration and unchanged-output migration.
The historical ASS payload contains the exact old wrappers independently of the
normalizer being tested. Before the fix all three tests failed: the affected
cases hit the real collision, and the control reused the old recipe identity.

The tests require the same reviewed input identity, a distinct current edition,
exact corrected output, preservation of nine protected files, idempotent rebuilds,
and refusal of missing or stale review evidence without publishing a ZIP. Their
historical render marker is explicitly pending; it is not fabricated approval.
The genuine rendered-and-approved upgrade is verified separately with the real
old and fixed CLI executables. Existing historical migration coverage is extended
to include `19/16`, alongside `13/12`, `17/14` and `18/15`.

These tests run in the existing workspace command on all four native CI hosts.
The repair PR must carry final-head results for the release build, format check,
Clippy, full tests, workflow lint and security checks. After any code change, use
that new head's results. Publish the actual CLI migration receipt, including fresh
image inspection, QA, package readback and preservation checks, in the final PR
receipt. Stop the accepted audit scope after these criteria and independent
review pass; do not restart discovery without a concrete failing observation.

The fixed optimized CLI (`6eec4a9b94da2bbdc06f00bdb12deaccf149741a8aa337c2ffb8e9da7c2a5d38`)
completed all four real same-workspace migrations. Thirty-one previous workspace
files and four original sources remained byte-identical. Rebuilds were idempotent;
missing or copied old evidence was refused. A newly rendered image was opened
and inspected, a fresh observation recorded, and QA then passed. The new ZIP had
exactly `Sub/S01E01.ass`, passed its CRC check and matched the current subtitle
bytes; a second publication to that occupied destination was refused without
changing the ZIP. The hard-space replacement identity is
`7adee55f74f6eb3d387d903079e2532a7ee6f4b3281d40dc4993175b6f656665`.

## Passing changed-area controls

The optimized baseline passed 120 exact conversion cases: 20 header, inline
`q`, Banner and context configurations, three ASS escapes, and with/without a
preceding hard display line. Every case preserved its source bytes and remained
idempotent on a second conversion. A separate 36-case reviewed-output matrix and
40 retained soft-space/FSI controls also passed, as did 16 narrow-budget wrapping
cases. All 212 semantic cases passed; all 136 conversion cases were idempotent
and source-preserving. Four FFmpeg/libass actual/expected/literal-equivalent
triplets produced 12 nonblank PNGs with identical RGB pixels within each triplet;
two actual frames were also opened for inspection.

One initial test expectation incorrectly treated Persian digits as non-LTR.
Their Unicode bidi class is `EN`, which the documented mixed-text policy
intentionally recognizes. Correcting that independent test oracle resolved the
observation; production code was not changed to satisfy the incorrect expectation.
Oversized `q` values and Banner initialization were checked against libass's
parser and were not accepted as defects.

All six workflows, release and installation contracts, Dependabot and live
repository metadata were reviewed. Gitleaks now uses an independent main-push
concurrency group per SHA. About already describes the Rust implementation;
the repository has 20 topics including `rust`, and the README tag badge uses a
dynamic GitHub endpoint. No additional functional defect was accepted there.

The latest published release at intake was still `v3.0.4`, targeting `ec3e869`.
Its assets are distinct from the later source-built implementations above; this
audit does not represent the repair as already released. The separate existing
human-author email mismatch in historical commit `95ad08b` requires the owner's
history-maintenance procedure. An ordinary source PR cannot rewrite that record.

## References

- [Immutable reviewed input and edition definitions](../CONTEXT.md)
- [Source build and supported commands](../README.md#build-from-source)
- [Verification contract](verification.md)
- [libass 0.17.3 parser](https://github.com/libass/libass/blob/0.17.3/libass/ass_parse.c)
- [Unicode Bidirectional Algorithm](https://www.unicode.org/reports/tr9/tr9-52.html)
- [Cargo test and `--no-fail-fast`](https://doc.rust-lang.org/cargo/commands/cargo-test.html)
- [GitHub workflow concurrency](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency)
- [Dependabot options](https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference)
- [GitHub topics and the 20-topic limit](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/classifying-your-repository-with-topics)
- [Dynamic Shields GitHub Tag badge](https://shields.io/badges/git-hub-tag)

Measured automated coverage and authored fixtures do not prove correctness for
every possible media file or replace editorial review of real subtitle content.
