# Platform references

Discovery paths and five plugin manifest formats were re-checked against current
official documentation and `cloudflare/skills` on 2026-11-24. Distribution
compatibility is not a claim that every host application was installed or exercised.
A skill must run with the host's filesystem, shell, web and image-viewing permissions.

| Host | Official references |
| --- | --- |
| Claude Code | [Skills](https://code.claude.com/docs/en/skills), [Plugins](https://code.claude.com/docs/en/plugins-reference) |
| Codex | [Skills](https://learn.chatgpt.com/docs/build-skills), [Plugins](https://learn.chatgpt.com/docs/build-plugins) |
| Cursor | [Skills](https://cursor.com/docs/skills), [Plugins](https://cursor.com/docs/plugins) |
| Kiro | [Skills](https://kiro.dev/docs/skills/), [Powers](https://kiro.dev/docs/powers/) |
| Cline | [Skills](https://docs.cline.bot/customization/skills) |
| Hermes | [Skills and trust](https://hermes-agent.nousresearch.com/docs/user-guide/features/skills), [Plugins](https://hermes-agent.nousresearch.com/docs/developer-guide/plugins) |
| OpenCode | [Skills](https://opencode.ai/docs/skills/) |
| Antigravity IDE | [Skills](https://www.antigravity.google/docs/ide/skills/), [Plugins](https://www.antigravity.google/docs/ide/plugins/) |
| Antigravity CLI | [CLI plugins and skills](https://www.antigravity.google/docs/cli/plugins/) |

The repository ships all five plugin manifest formats at version `2.0.0`, registering
both `revayat-subtitle` and `revayat-subtitle-sync`:

- **Claude Code:** `.claude-plugin/plugin.json` and `.claude-plugin/marketplace.json` (skills and four Markdown commands).
- **Cursor:** `.cursor-plugin/plugin.json` and `.cursor-plugin/marketplace.json` (skills and four Markdown commands).
- **Codex:** `.codex-plugin/plugin.json` and `.agents/plugins/marketplace.json` (`./skills/` discovery).
- **Antigravity:** `.antigravity-plugin/plugin.json` (skills and suggested prompts; Antigravity ignores `commands/`).
- **Agent Plugins 1.0.0:** root `plugin.json` (convention discovery under `skills/`).

The installer supports a destination override for managed installations and future path
changes. Python 3.11+ (verified on 3.13.15) is the supported Python floor, and Rust
(verified on 1.99.1; 1.88+ for `rust/subsync-rs`, 1.97+ for PyO3 crates) builds the
`rust/subsync-rs` audio-to-subtitle synchronization CLI and the optional PyO3
accelerators (`rust/subcore-rs`, `rust/persian-lint-rs`, and `rust/markup-rs`). Older
dated 3.10 verification records remain historical, not current support.

Standard agent installations place new recovery state in the first selected
agent root's `revayat-recovery/`, outside skills/plugin discovery. Custom
`--destination` requires explicit same-filesystem `--recovery-dir`; configure it
outside every host scan, not merely a hidden sibling. Existing backups remain
untouched. One retained native nonblocking lock under
`~/revayat-install-state/` serializes same-account installs/recovery across all
destinations/recovery roots. It is not cross-account or network-filesystem
certification. See [recovery](../skills/revayat-subtitle/references/troubleshooting.md#installer-process-interruption).
