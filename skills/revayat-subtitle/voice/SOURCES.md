# Bundled voice skills

The [voice router](../references/target-voice.md) uses the following MIT-licensed files. Each skill directory includes its own license. The Persian directory also retains the upstream `NOTICE` for its seed data.

| Skill | Pinned upstream | Bundled files |
| --- | --- | --- |
| Humanizer | [blader/humanizer v4.0.0](https://github.com/blader/humanizer/tree/b48c832f0f809c1b5658edbb838c2521b72f2413) | Skill and license. The upstream plugin, validator and developer files are not needed to edit text. |
| Great Russian | [frolin/great-russian-skill at c9ba866](https://github.com/frolin/great-russian-skill/tree/c9ba8661768927c21b7ba5608e63dce832c568b5) | Skill, license and references. |
| Persian | [Mojtaba-Alehosseini/persian-skill at 4264f6e](https://github.com/Mojtaba-Alehosseini/persian-skill/tree/4264f6ef03f7568bc15b14287a3db09bc815fc31) | Skill, license, notice, references, optional utilities and their data and tests. |

Update the pin and installer allowlist together when an upstream adds or removes a required file. Revayat's subtitle workflow takes precedence over generic editing instructions.
