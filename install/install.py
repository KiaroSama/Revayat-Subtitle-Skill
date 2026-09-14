"""Install portable Revayat skills or a plugin bundle using explicit file allowlists."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import logging
from pathlib import Path
import shutil
import sys

REPO = Path(__file__).resolve().parent.parent
SKILL = REPO / "skills" / "revayat-subtitle"
sys.path.insert(0, str(SKILL / "scripts"))
from runtime import operational_log, staging_directory

NAME = "revayat-subtitle"
AGENTS = ("claude", "codex", "cursor", "kiro", "cline", "hermes", "opencode", "antigravity", "antigravity-cli")


def destination(agent: str, scope: str, base: Path) -> Path:
    folder = {"claude": ".claude", "codex": ".agents", "cursor": ".cursor", "kiro": ".kiro",
              "cline": ".cline", "hermes": ".hermes", "opencode": ".opencode",
              "antigravity": ".agents", "antigravity-cli": ".agents"}[agent]
    if scope == "user":
        folder = {"opencode": ".config/opencode", "antigravity": ".gemini/antigravity",
                  "antigravity-cli": ".gemini/antigravity-cli"}.get(agent, folder)
    return base / folder / "skills" / NAME


def skill_files() -> list[Path]:
    files = [SKILL / "SKILL.md", SKILL / "LICENSE"]
    for folder, suffixes in {"scripts": {".py"}, "references": {".md"}, "agents": {".yaml"}}.items():
        files.extend(sorted(path for path in (SKILL / folder).iterdir() if path.suffix in suffixes))
    if any(not path.is_file() or path.is_symlink() for path in files):
        raise ValueError("Skill distribution contains a missing file or symlink")
    return files


def install(target: Path, *, plugin: bool, force: bool) -> Path | None:
    target = target.absolute()
    if target.exists() and (target.is_symlink() or getattr(target.lstat(), "st_file_attributes", 0) & 1024):
        raise ValueError("Refusing to replace a linked destination")
    if target.exists() and not force:
        raise ValueError("Destination exists; use --force to keep a backup and replace this skill")
    if target.resolve() == REPO or target.resolve() == SKILL or REPO.is_relative_to(target.resolve()):
        raise ValueError("Installation destination overlaps the source repository")
    protected = [SKILL, REPO / "install", REPO / "commands", REPO / ".codex-plugin",
                 REPO / ".claude-plugin", REPO / ".cursor-plugin"]
    if any(target.resolve().is_relative_to(path) or path.is_relative_to(target.resolve()) for path in protected):
        raise ValueError("Installation destination overlaps distribution source files")
    files = skill_files()
    pairs = [(path, path.relative_to(SKILL)) for path in files]
    if plugin:
        pairs = [(path, Path("skills") / NAME / path.relative_to(SKILL)) for path in files]
        for relative in ("plugin.json", "LICENSE", ".codex-plugin/plugin.json", ".claude-plugin/plugin.json",
                         ".claude-plugin/marketplace.json", ".cursor-plugin/plugin.json",
                         "commands/translate-subtitles.md"):
            source = REPO / relative
            if not source.is_file() or source.is_symlink():
                raise ValueError("Plugin distribution is incomplete or contains a symlink")
            pairs.append((source, Path(relative)))
    target.parent.mkdir(parents=True, exist_ok=True)
    parent = target.parent.resolve()
    if target.resolve().parent != parent:
        raise ValueError("Destination escapes its installation directory")
    backup = None
    with staging_directory(parent, ".revayat-install-") as temporary:
        stage = Path(temporary) / NAME
        stage.mkdir()
        for source, relative in pairs:
            output = stage / relative
            output.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, output)
        if target.exists():
            stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
            backup = parent / (target.name + ".backup-" + stamp)
            if backup.parent != parent or target.resolve().parent != parent:
                raise ValueError("Backup move escapes its installation directory")
            target.rename(backup)
        try:
            stage.rename(target)
        except BaseException:
            if backup and not target.exists():
                backup.rename(target)
            raise
    logging.info("Installed mode=%s", "plugin" if plugin else "skill")
    return backup


def main(argv=None) -> int:
    with operational_log("install"):
        parser = argparse.ArgumentParser(description=__doc__)
        parser.add_argument("--agent", choices=(*AGENTS, "all"), default="all")
        parser.add_argument("--scope", choices=("user", "project"), default="user")
        parser.add_argument("--path", type=Path, default=Path.cwd(), help="Existing project root for project scope")
        parser.add_argument("--destination", type=Path, help="Exact skill/plugin directory; overrides agent routing")
        parser.add_argument("--plugin", action="store_true", help="Copy the complete plugin to --destination")
        parser.add_argument("--force", action="store_true", help="Back up an existing installation before replacing it")
        parser.add_argument("--dry-run", action="store_true")
        args = parser.parse_args(argv)
        try:
            if args.plugin and not args.destination:
                raise ValueError("Plugin installation needs an explicit --destination")
            base = Path.home() if args.scope == "user" else args.path.resolve()
            if not base.is_dir():
                raise ValueError("Installation base directory does not exist")
            targets = [args.destination] if args.destination else []
            if not targets:
                for agent in AGENTS if args.agent == "all" else (args.agent,):
                    target = destination(agent, args.scope, base)
                    if args.agent == "all" and args.scope == "user":
                        present = target.parent.parent.is_dir()
                        if agent == "codex":
                            present = present or (base / ".codex").is_dir()
                        if not present:
                            print(f"Skipped {agent}: no existing agent directory")
                            continue
                    if target not in targets:
                        targets.append(target)
            if not targets:
                raise ValueError("No installed agents found; select --agent or --destination explicitly")
            # Fail before changing any target when replacement permission is missing.
            if not args.force and not args.dry_run and any(target.exists() for target in targets):
                raise ValueError("An installation already exists; use --force to back it up before replacement")
            for target in targets:
                if args.dry_run:
                    print(f"Would install: {target}")
                    continue
                backup = install(target, plugin=args.plugin, force=args.force)
                print(f"Installed: {target}")
                if backup:
                    print(f"Previous installation retained: {backup}")
            logging.info("Installer completed exit=0 targets=%d", len(targets))
            return 0
        except (OSError, ValueError) as error:
            logging.error("Installer failed error_type=%s exit=2", type(error).__name__)
            print(f"ERROR: {error}", file=sys.stderr)
            return 2


if __name__ == "__main__":
    sys.exit(main())
