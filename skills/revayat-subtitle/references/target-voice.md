# Voice by output language

Use `project.json.target_language` for every editorial batch, including resumed batches and worker assignments. `prepare` defaults to `fa`. For a region or script variant, match the primary subtag case-insensitively: `fa-IR` uses `persian`, `ru-RU` uses `great-russian`, and `en-GB` uses `humanizer`. The source and chat languages do not select a guide.

| Target | Bundled skill | Claude plugin key |
| --- | --- | --- |
| `fa` | [persian](../voice/persian/SKILL.md) | `revayat-subtitle:persian` |
| `ru` | [great-russian](../voice/great-russian/SKILL.md) | `revayat-subtitle:great-russian` |
| `en` | [humanizer](../voice/humanizer/SKILL.md) | `revayat-subtitle:humanizer` |

Before drafting or correcting cues, load only the matching bundled skill. If the host does not expose its plugin key, read its linked `SKILL.md` and the references it calls for. If an expected guide is missing, report it and use the subtitle policy without claiming you applied the guide. For other targets, use the subtitle policy and the researched conventions of that language.

Draft for meaning, then edit for spoken rhythm and the character's register. Use the guide on editable dialogue or display text only. Preserve cue IDs, timings, tags, vector paths, glossary entries, deliberate repetition and the speaker's intent. Do not paste draft variants, editorial commentary or headings into the worksheet. If polishing changes meaning or intensity, return to the source before accepting the wording. Apply [Persian typography](persian-typography.md) to Persian spans, including Persian inside a different target.

Use upstream utilities only when needed and after checking their scope. Never run a prose linter on the whole ASS/SRT file or let it modify protected spans. Do not install another plugin, call an external editing service or upload subtitle text to apply these guides.
