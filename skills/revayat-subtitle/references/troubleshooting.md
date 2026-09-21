# Troubleshooting and recovery

Use this when a command refuses a workspace/build or the rendered result is wrong.

| Message or symptom | What to do |
| --- | --- |
| `FFmpeg not found` | Locate a libass-enabled FFmpeg; use `--ffmpeg` or `REVAYAT_FFMPEG`. |
| Invalid encoding, timestamp or ASS Format | Inspect and repair a separate copy, then import with the confirmed encoding. |
| `Work directory already exists` | Resume its existing worksheets; choose a new directory only for a new import. |
| Source inventory/hash differs | Recover the imported files and inventory or deliberately start a fresh workspace. |
| Cue not reviewed, or original IDs missing | Read and restore the complete worksheet; never synthesize approval flags. |
| Invalid alternate link | Link to the retained cue(s) in the same episode that actually cover its meaning. |
| Undefined style or incompatible ASS canvas/fonts | Reconcile presentation explicitly before merging. |
| Missing timing/structure note | Verify the edit and record why it is necessary; render it again. |
| Workspace/build/render changed | Rebuild from the current worksheets and inspect fresh evidence. |
| Missing or unreviewed frame | Render that episode and inspect every required PNG before signing its review. |
| Broken Persian, English order, quotes or sign tracking | Follow [persian-typography.md](persian-typography.md) using the raw string and image together. |
| Output ZIP already exists | Select a new filename; the original delivery is preserved. |
| Legacy build | Rebuild its existing workspace with the current recipe and rerender; preserve old editions and decisions. |
| JSON field/schema error | Repair the named field, duplicate key or type; do not replace missing evidence with invented defaults. |
| Installation rollback incomplete | Preserve the exact target/backup paths reported; resolve locks or external changes before restoring. |
| Publication cleanup warning with committed=true | The complete output exists; verify it and retain the named staging file until safe cleanup, rather than overwriting or retrying the same filename. |
| Resource or output limit | Split a large job or inspect the failing tool; do not treat partial output as success. |

## Resume without losing decisions

Read the existing `project.json`, glossary and worksheets, and identify the first
pending cue. Reuse completed decisions and the established series identity.
If only visual review is pending, inspect those images. If text changes, follow
the new build path printed by `build`; old reviews belong to old bytes.

`qa --build BUILD` runs the delivery checks without creating a ZIP or changing
editorial approvals. A failure is a finding, not an instruction to delete data.
After a render timeout, inspect the failure and any diagnostic images before retrying;
each FFmpeg child is bounded to 45 seconds and a timeout is not success.

## CLI execution logs

Every run writes a new UTF-8 file inside the skill's `logs/`, named
`revayat-subtitle_YYYY-MM-DD_HH-mm-ss_UTC.log`; installers use `install_...`.
Collisions receive a suffix. Entries use UTC `[timestamp] [LEVEL] [COMPONENT] Message`.
`REVAYAT_LOG_DIR` overrides the directory; read-only installations fall back to stderr.

Logs contain stage progress, error types and exit status, never subtitle bodies.
`REVAYAT_LOG_LEVEL` accepts DEBUG, INFO (default), WARNING, ERROR and CRITICAL.
DEBUG adds bounded lifecycle diagnostics. Invalid levels fail without echoing their
contents. Native installer bootstrap logs cover missing Python; Windows supervisors
also record their target's actual failure status. Automatic log deletion is disabled.
Inspect local path information
before sharing logs and remove old files when no longer needed.

Missing shell, web search, image viewing, video or fonts must be reported accurately.
Neutral-background rendering does not prove audiovisual synchronization. File and
image hashes do not certify translation quality or that a reviewer actually read them.
