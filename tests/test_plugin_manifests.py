"""Check plugin registrations and the installer payloads they depend on."""

import contextlib
import io
import json
import unittest
from pathlib import PurePosixPath, PureWindowsPath

from test_installation import ROOT, WorkspaceCase, installer

NAME = "revayat-subtitle"
VERSION = "2.0.0"
LICENSE = "GPL-3.0-or-later"
REPOSITORY = "https://github.com/KiaroSama/Revayat-Subtitle-Skill"
AUTHOR = {"name": "Kiaro Sama", "email": "Kiaro.Sama.Dev@gmail.com"}
PORTABLE = "plugin.json"
CLAUDE = ".claude-plugin/plugin.json"
CURSOR = ".cursor-plugin/plugin.json"
CODEX = ".codex-plugin/plugin.json"
ANTIGRAVITY = ".antigravity-plugin/plugin.json"
CATALOGS = {
    ".claude-plugin/marketplace.json": CLAUDE,
    ".cursor-plugin/marketplace.json": CURSOR,
    ".agents/plugins/marketplace.json": CODEX,
}
SCHEMAS = {
    PORTABLE: "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
    ANTIGRAVITY: "https://antigravity.google/schemas/v1/plugin.json",
    ".claude-plugin/marketplace.json": "https://code.claude.com/schemas/marketplace.json",
}
PLUGINS = (PORTABLE, CLAUDE, CURSOR, CODEX, ANTIGRAVITY)
SKILLS = {f"./skills/{NAME}", f"./skills/{NAME}-sync"}
VOICE_SKILLS = {f"./skills/{NAME}/voice/{name}" for name in ("humanizer", "great-russian", "persian")}
COMMANDS = {
    "./commands/translate-subtitles.md",
    "./commands/revayat-subtitle-qa.md",
    "./commands/revayat-subtitle-resume.md",
    "./commands/revayat-subtitle-sync.md",
}
SUPPORT_FILES = (
    "sync/SKILL.md", "references/audio-sync.md", "scripts/sync_bridge.py",
    "assets/fonts/BKOODB.TTF", "assets/fonts/NOTICE.txt",
)


def manifest(relative):
    return json.loads((ROOT / relative).read_text(encoding="utf-8"))


class TestPluginManifests(unittest.TestCase):
    def assert_metadata(self, data):
        self.assertEqual(data["name"], NAME)
        self.assertEqual(data["version"], VERSION)
        self.assertEqual(data["license"], LICENSE)
        self.assertEqual(data["repository"], REPOSITORY)
        self.assertEqual({key: data["author"][key] for key in AUTHOR}, AUTHOR)

    def contained(self, reference, *, must_exist=True):
        self.assertTrue(reference.startswith("./"), reference)
        self.assertNotIn("\\", reference)
        self.assertFalse(PureWindowsPath(reference[2:]).anchor, reference)
        self.assertNotIn("..", PurePosixPath(reference).parts, reference)
        path = (ROOT / reference).resolve(strict=must_exist)
        self.assertTrue(path.is_relative_to(ROOT.resolve()), reference)
        return path

    def test_plugin_metadata_and_schema_urls(self):
        for relative in PLUGINS:
            with self.subTest(manifest=relative):
                data = manifest(relative)
                self.assert_metadata(data)
                self.assertEqual(data.get("$schema"), SCHEMAS.get(relative))

    def test_marketplaces_point_to_the_release(self):
        for relative, plugin in CATALOGS.items():
            with self.subTest(catalog=relative):
                data = manifest(relative)
                self.assertEqual(data.get("$schema"), SCHEMAS.get(relative))
                self.assertEqual(len(data["plugins"]), 1)
                entry = data["plugins"][0]
                self.assertEqual(entry["name"], NAME)
                if plugin == CODEX:
                    self.assertEqual(entry["source"], {"source": "url", "url": f"{REPOSITORY}.git"})
                    self.assertEqual(entry["policy"], {"installation": "AVAILABLE", "authentication": "NONE"})
                else:
                    self.assertEqual(self.contained(entry["source"]), ROOT.resolve())
                    self.assertEqual({key: data["owner"][key] for key in AUTHOR}, AUTHOR)
                    self.assert_metadata(entry)
                self.assert_metadata(manifest(plugin))

    def test_declared_paths_stay_inside_repository(self):
        for relative in PLUGINS:
            with self.subTest(manifest=relative):
                data = manifest(relative)
                for component in ("skills", "commands"):
                    paths = data.get(component, [])
                    for path in [paths] if isinstance(paths, str) else paths:
                        self.contained(path, must_exist=False)

    def test_skill_registrations_resolve(self):
        for relative in PLUGINS:
            with self.subTest(manifest=relative):
                data = manifest(relative)
                if relative == PORTABLE:
                    self.assertNotIn("skills", data)
                    skills = SKILLS
                elif relative == CODEX:
                    self.assertEqual(data["skills"], "./skills/")
                    self.assertEqual(self.contained(data["skills"]), ROOT.resolve() / "skills")
                    skills = SKILLS
                else:
                    skills = SKILLS | VOICE_SKILLS if relative == CLAUDE else SKILLS
                    self.assertCountEqual(data["skills"], skills)
                for skill in skills:
                    self.assertTrue(self.contained(f"{skill}/SKILL.md").is_file(), skill)

    def test_command_registrations_match_host_support(self):
        for relative in PLUGINS:
            with self.subTest(manifest=relative):
                data = manifest(relative)
                if relative in (CLAUDE, CURSOR):
                    self.assertCountEqual(data["commands"], COMMANDS)
                    for command in data["commands"]:
                        self.assertTrue(self.contained(command).is_file(), command)
                else:
                    self.assertNotIn("commands", data)
        antigravity = manifest(ANTIGRAVITY)
        self.assertTrue(antigravity["category"])
        self.assertTrue(any("sync" in prompt.lower() for prompt in antigravity["suggestedPrompts"]))


class TestPluginInstallation(WorkspaceCase):
    def test_standard_project_install_publishes_both_skills(self):
        module = installer()
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            result = module.main(["--agent", "claude", "--scope", "project", "--path", str(self.root)])
        self.assertEqual(result, 0)
        skills = self.root / ".claude/skills"
        self.assertEqual({path.name for path in skills.iterdir()}, {NAME, f"{NAME}-sync"})
        for relative in (f"{NAME}/SKILL.md", f"{NAME}-sync/SKILL.md", *(f"{NAME}/{file}" for file in SUPPORT_FILES)):
            self.assertEqual((skills / relative).read_bytes(), (ROOT / "skills" / relative).read_bytes(), relative)

    def test_plugin_install_includes_every_manifest_skill_and_command(self):
        target = self.root / "plugin"
        installer().install(target, plugin=True, force=False, recovery_dir=self.root / "recovery")
        payload = [*PLUGINS, *CATALOGS, *COMMANDS, *(f"{skill}/SKILL.md" for skill in SKILLS),
                   *(f"skills/{NAME}/{file}" for file in SUPPORT_FILES)]
        for relative in payload:
            self.assertEqual((target / relative).read_bytes(), (ROOT / relative).read_bytes(), relative)
