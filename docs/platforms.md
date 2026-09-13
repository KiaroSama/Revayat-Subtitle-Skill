# Platform references

Discovery paths and plugin formats were checked against current official
documentation on 2026-09-14. Distribution compatibility is not a claim that every
host application was installed or exercised. A skill must run with the host's
filesystem, shell, web and image-viewing permissions.

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

The root manifest follows [Agent Plugins](https://agent-plugins.org/plugin-authors/manifest);
its `skills/` location is fixed discovery, not a `skills` field. Compatibility
manifests use the individual host contracts. Codex metadata was also checked with
the installed official plugin validator. The installer supports a destination
override for managed installations and future path changes.
