# Verification scope

## Automated checks

The bounded stdlib runner exercises the actual CLI and OS installer, complete cue
accounting, candidate references, stale source/output/manifest/glossary refusal,
safe ZIP import, explicit encoding failures, ASS reset styles, opaque embedded
font blocks, drawing preservation, SRT sorting and OVA names, license inclusion,
installation source protection and replacement backups. CI covers Python 3.10
and 3.14 on Linux, and Python 3.14 on Windows and macOS.

The Linux render tier additionally checks that subsecond samples contain actual
caption pixels, 10-bit video produces viewable 8-bit PNGs, incomplete visual
reviews prevent packaging, image modifications invalidate approval, and ZIP bytes
match the reviewed edition. Automated signoffs in this test are deliberately
labelled gate-contract fixtures, not human visual judgments. CodeQL analyzes
Python and workflows. Dependency Review is triggered on pull requests; Dependabot
maintains GitHub Actions and its security updates are enabled for the repository.

Use the [Actions results](https://github.com/KiaroSama/Revayat-Subtitle-Skill/actions)
for the result on the commit being evaluated; configuration alone is not a pass.

## Real local media, 2026-09-14

Four real ASS candidates, including two Persian releases and two English tracks
extracted from their matching MKV, contained 3,559, 655, 370 and 604 events.
All 5,188 passed parse/serialize/parse structural round trips. This checks file
mechanics; it does not claim a semantic review of all those events.

A bounded opening excerpt contained 40 source events across those candidates.
Every sampled event was read and assigned a decision. Ten retained events kept
intentional foreground/shadow layers and a moving translated sign. All ten
rendered images were inspected over the actual 10-bit video, with corrected
Persian/English order, punctuation, shaping and sign tracking. Packaging was read
back and the source subtitle hashes remained unchanged. FFmpeg 9.0.1 and Python
3.13.15 were used on Windows. Only authored fixtures are included in Git.

The real case exposed and informed fixes for inherited owner-only Windows output
permissions, fractional timestamp truncation and 16-bit PNG previews. Its source
typography also required a Persian font, resetting inherited negative tracking,
and libass whole-text layout for two sign layers with separately scaled numbers.
The latter is a libass-specific adaptation; VSFilter compatibility was not tested.
The skill documents these as decisions to make after actual image inspection.

## Limits

- No full episode/season semantic certification or audiovisual synchronization audit.
- No installation and execution inside every supported host application's UI.
- Embedded font payload preservation/removal has mechanical fixtures; the real
  selected ASS files had no embedded font sections. No claim of exhaustive
  embedded-font renderer compatibility is made.
- Codebase Memory enrollment was repaired for the exact project root. Its index
  is local development tooling, separate from the distributed subtitle runtime.

The private input media, extracted subtitles, review worksheets and image evidence
stay local and are excluded from skill/plugin distributions and CI uploads.
