# Platform references

## Supported platforms

Revayat Subtitle `3.0.1` builds from Rust `1.100.0`, edition `2024`. The [CI workflow](../.github/workflows/ci.yml) and [release workflow](../.github/workflows/release.yml) target these 64-bit platforms only:

| OS | Architecture | Rust target | Local binaries |
| --- | --- | --- | --- |
| Windows | x64 | `x86_64-pc-windows-msvc` | `bin/revayat-subtitle.exe`, `bin/subsync.exe` |
| Windows | arm64 | `aarch64-pc-windows-msvc` | `bin/revayat-subtitle.exe`, `bin/subsync.exe` |
| Linux | x64 | `x86_64-unknown-linux-gnu` | `bin/revayat-subtitle`, `bin/subsync` |
| Linux | arm64 | `aarch64-unknown-linux-gnu` | `bin/revayat-subtitle`, `bin/subsync` |

The [source build instructions](../README.md#build-from-source) use PowerShell 7 on Windows and a POSIX shell on Linux. For media work, put `ffmpeg` and `ffprobe` on `PATH`. Use FFmpeg with libass and the `subtitles` filter for rendering. The project bundles the `B Koodak` subtitle font and does not bundle FFmpeg. See [verification scope](verification.md) for which checks run on each target.

## Agent installation

Run the installer from a checkout of the matching release. It reads the skill tree, commands, and manifests from the checkout. Run `revayat-subtitle install --dry-run` to see the destinations, then `revayat-subtitle install` to copy the skills into existing user roots. Use `--agent NAME` to select a host or `--agent NAME --scope project --path PATH` for an explicit project install. Reload the host after installing and make both binaries available on its `PATH`.

| Host | User skills | Project skills | Host documentation |
| --- | --- | --- | --- |
| Claude Code | `~/.claude/skills/` | `.claude/skills/` | [Skills](https://code.claude.com/docs/en/skills), [Plugins](https://code.claude.com/docs/en/plugins-reference) |
| Codex | `~/.agents/skills/` | `.agents/skills/` | [Skills](https://learn.chatgpt.com/docs/build-skills), [Plugins](https://learn.chatgpt.com/docs/build-plugins) |
| Cursor | `~/.cursor/skills/` | `.cursor/skills/` | [Skills](https://cursor.com/docs/skills), [Plugins](https://cursor.com/docs/plugins) |
| Kiro | `~/.kiro/skills/` | `.kiro/skills/` | [Skills](https://kiro.dev/docs/skills/), [Powers](https://kiro.dev/docs/powers/) |
| Cline | `~/.cline/skills/` | `.cline/skills/` | [Skills](https://docs.cline.bot/customization/skills) |
| Hermes | `~/.hermes/skills/` | `.hermes/skills/` | [Skills and trust](https://hermes-agent.nousresearch.com/docs/user-guide/features/skills), [Plugins](https://hermes-agent.nousresearch.com/docs/developer-guide/plugins) |
| OpenCode | `~/.config/opencode/skills/` | `.opencode/skills/` | [Skills](https://opencode.ai/docs/skills/) |
| Antigravity IDE | `~/.gemini/antigravity/skills/` | `.agents/skills/` | [Skills](https://www.antigravity.google/docs/ide/skills/), [Plugins](https://www.antigravity.google/docs/ide/plugins/) |
| Antigravity CLI (`antigravity-cli`) | `~/.gemini/antigravity-cli/skills/` | `.agents/skills/` | [CLI plugins and skills](https://www.antigravity.google/docs/cli/plugins/) |

The installer puts new recovery data in the first selected agent root's `revayat-recovery/`, outside skill discovery. When setting `--destination`, pass `--recovery-dir` on the same filesystem and outside every configured skill or plugin discovery root. A single OS lock under `~/revayat-install-state/` serializes installs and recovery for the same account. This does not certify cross-account or network-filesystem use. Existing backups stay in place. See [interrupted installation recovery](../skills/revayat-subtitle/references/troubleshooting.md#installer-process-interruption).

### Installer downgrade compatibility

The current installer reads legacy journals. An older installer can refuse newer Unix journals containing authenticated executable permissions with `Invalid installation tree manifest`, even after successful uninstall or recovery because their retained history still uses the newer format. This refusal preserves installed files and transaction journals. Before reaching an incompatible journal, the older installer may update or retire earlier completed entries in the account registry; interrupted transactions remain unchanged. It does not make the old executable compatible with that registry.

Before reverting runtime binaries or skill payloads, retain the compatible newer installer and its matching checkout outside the installed trees. Use that installer for existing uninstall/recovery operations with the original destination and recovery paths, and resolve interrupted transactions before installing another payload. Its `--source OLDER_CHECKOUT` option can select an older payload that passes the installer's normal source validation. Keep the compatible installer available while newer registry records, journals or backups remain. Never strip permission evidence, edit authentication fields, or delete recovery history to force an older installer to proceed.

## Plugin manifests

The five manifest formats share the `3.0.1` skill tree and provide `revayat-subtitle` and `revayat-subtitle-sync`:

- Claude Code: `.claude-plugin/plugin.json` and `.claude-plugin/marketplace.json` (skills and four Markdown commands).
- Cursor: `.cursor-plugin/plugin.json` and `.cursor-plugin/marketplace.json` (skills and four Markdown commands).
- Codex: `.codex-plugin/plugin.json` and `.agents/plugins/marketplace.json` (discovery under `skills/`).
- Antigravity: `.antigravity-plugin/plugin.json` (skills and suggested prompts; Antigravity ignores `commands/`).
- Agent Plugins 1.0: root `plugin.json` (discovery under `skills/`).

To prepare a local plugin bundle, use `revayat-subtitle install --plugin --destination "PATH/revayat-subtitle" --recovery-dir "PATH/recovery"`. Keep the recovery path outside discovery and on the same filesystem as the destination.

An install path or manifest describes how a host discovers the skill. It does not establish that the workflow has run inside every listed host. The host needs permission to read and write the selected workspace, run the CLI, research terminology, and view images for visual review.
