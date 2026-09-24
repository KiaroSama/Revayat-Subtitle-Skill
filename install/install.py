"""Install portable Revayat skills or a plugin bundle using explicit file allowlists."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import logging
import os
import errno
import uuid
from pathlib import Path
import shutil
import sys

REPO = Path(__file__).resolve().parent.parent
SKILL = REPO / "skills" / "revayat-subtitle"
sys.path.insert(0, str(SKILL / "scripts"))
from runtime import file_fingerprint, operational_log, output_directory, LogConfigurationError
from publication import rename_noreplace

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
    files = [SKILL / "SKILL.md", SKILL / "LICENSE", SKILL / "requirements.txt"]
    for folder, suffixes in {"scripts": {".py"}, "references": {".md"}, "agents": {".yaml"}}.items():
        files.extend(sorted(path for path in (SKILL / folder).iterdir() if path.suffix in suffixes))
    if any(not path.is_file() or path.is_symlink() for path in files):
        raise ValueError("Skill distribution contains a missing file or symlink")
    return files


def linked(path: Path) -> bool:
    return path.is_symlink() or bool(getattr(path.lstat(), "st_file_attributes", 0) & 1024)


def identity(path: Path):
    info = path.lstat()
    return info.st_dev, info.st_ino


def payload(plugin: bool):
    pairs = [(path, path.relative_to(SKILL)) for path in skill_files()]
    if plugin:
        pairs = [(path, Path("skills") / NAME / relative) for path, relative in pairs]
        for relative in ("plugin.json", "LICENSE", ".codex-plugin/plugin.json", ".claude-plugin/plugin.json",
                         ".claude-plugin/marketplace.json", ".cursor-plugin/plugin.json",
                         "commands/translate-subtitles.md", "commands/revayat-subtitle-resume.md",
                         "commands/revayat-subtitle-qa.md"):
            source = REPO / relative
            if not source.is_file() or linked(source):
                raise ValueError("Plugin distribution contains a missing or linked file")
            pairs.append((source, Path(relative)))
    return pairs


def plan_install(targets: list[Path], *, plugin: bool, force: bool) -> dict:
    if type(plugin) is not bool or type(force) is not bool:
        raise ValueError("Installation flags must be booleans")
    pairs = payload(plugin)
    planned, seen, ancestors = [], set(), {}
    protected = [SKILL, REPO / "install", REPO / "commands", REPO / ".codex-plugin",
                 REPO / ".claude-plugin", REPO / ".cursor-plugin"]
    for original in targets:
        original = original.absolute()
        for path in (original, *original.parents):
            if os.path.lexists(path):
                if linked(path):
                    raise ValueError("Installation paths must not contain links or junctions")
                if not path.is_dir():
                    raise ValueError(f"Installation destination or parent is not a directory: {path}")
        target = original.resolve()
        for parent in target.parents:
            ancestors[parent] = identity(parent) if os.path.lexists(parent) else None
        if target == REPO or REPO.is_relative_to(target):
            raise ValueError("Installation destination overlaps the source repository")
        if any(target.is_relative_to(p) or p.is_relative_to(target) for p in protected):
            raise ValueError("Installation destination overlaps distribution source files")
        key = os.path.normcase(str(target))
        if key in seen:
            continue
        if any(target.is_relative_to(item["target"]) or item["target"].is_relative_to(target) for item in planned):
            raise ValueError("Installation targets overlap each other")
        seen.add(key)
        if target.exists() and not force:
            raise ValueError("An installation already exists; use --force to retain a backup")
        planned.append({"target": target, "previous": identity(target) if target.exists() else None})
    if not planned:
        raise ValueError("No installation targets selected")
    return {"targets": planned, "files": pairs, "ancestors": ancestors}


def check_parents(parent: Path, expected: dict):
    for path in (parent, *parent.parents):
        current = identity(path) if os.path.lexists(path) else None
        if current != expected[path] or (current is not None and (linked(path) or not path.is_dir())):
            raise ValueError(f"Installation ancestor changed after preflight: {path}")


def make_parents(parent: Path, created: list, expected: dict):
    missing = []
    while not parent.exists():
        missing.append(parent)
        parent = parent.parent
    for path in reversed(missing):
        check_parents(path.parent, expected)
        path.mkdir()
        expected[path] = identity(path)
        created.append((path, expected[path]))


def tree_matches(root: Path, expected: dict) -> bool:
    directories = {parent.as_posix() for name in expected for parent in Path(name).parents if parent != Path(".")}
    pending, found, seen_dirs = [root], set(), set()
    while pending:
        with os.scandir(pending.pop()) as entries:
            for entry in entries:
                path = Path(entry.path)
                name = path.relative_to(root).as_posix()
                if linked(path):
                    return False
                if entry.is_dir(follow_symlinks=False):
                    if name not in directories:
                        return False
                    seen_dirs.add(name)
                    pending.append(path)
                elif entry.is_file(follow_symlinks=False) and name in expected:
                    size, sha = expected[name]
                    if entry.stat(follow_symlinks=False).st_size != size:
                        return False
                    if file_fingerprint(path, size)["sha256"] != sha:
                        return False
                    found.add(name)
                else:
                    return False
        if len(found) + len(seen_dirs) > len(expected) + len(directories):
            return False
    return found == set(expected) and seen_dirs == directories


def execute_plan(plan: dict) -> list[Path | None]:
    records, parents, failures = [], [], []
    expected = {}
    for source, relative in plan["files"]:
        fingerprint = file_fingerprint(source, 16 * 1024 * 1024)
        expected[relative.as_posix()] = (fingerprint["bytes"], fingerprint["sha256"])
    ancestors = dict(plan["ancestors"])
    try:
        for item in plan["targets"]:
            target = item["target"]
            check_parents(target.parent, ancestors)
            make_parents(target.parent, parents, ancestors)
            check_parents(target.parent, ancestors)
            stage = output_directory(target.parent, ".revayat-install-")
            record = {**item, "stage": stage, "stage_id": identity(stage), "backup": None}
            records.append(record)
            for source, relative in plan["files"]:
                output = stage / relative
                output.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, output)
            if not tree_matches(stage, expected):
                raise ValueError("Installation source changed during staging")
        for record in records:
            target, stage = record["target"], record["stage"]
            check_parents(target.parent, ancestors)
            current = identity(target) if os.path.lexists(target) else None
            if current != record["previous"]:
                raise ValueError("Installation target changed after preflight")
            if current is not None:
                stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S")
                backup = target.with_name(target.name + ".backup-" + stamp + "-" + uuid.uuid4().hex[:8])
                record["backup"] = backup
                check_parents(target.parent, ancestors)
                rename_noreplace(target, backup)
            check_parents(target.parent, ancestors)
            rename_noreplace(stage, target)
        logging.info("Installed targets=%d", len(records))
        return [record["backup"] for record in records]
    except BaseException as original:
        for record in reversed(records):
            target, backup = record["target"], record["backup"]
            try:
                check_parents(target.parent, ancestors)
                if os.path.lexists(target) and identity(target) == record["stage_id"]:
                    if not tree_matches(target, expected):
                        raise ValueError("Published installation changed; preserve it for recovery")
                    shutil.rmtree(target)
                if backup and os.path.lexists(backup):
                    if linked(backup) or not backup.is_dir() or identity(backup) != record["previous"]:
                        raise ValueError("Backup identity changed")
                    rename_noreplace(backup, target)
            except (OSError, ValueError):
                failures.append(f"target={target}; backup={backup}")
        if failures:
            raise ValueError("Installation rollback incomplete; preserve recovery paths: " + " | ".join(failures)) from original
        raise
    finally:
        for record in records:
            stage = record["stage"]
            try:
                check_parents(stage.parent, ancestors)
            except ValueError:
                logging.error("Installation stage ancestor changed; preserve recovery path: %s", stage)
                continue
            if os.path.lexists(stage):
                if not linked(stage) and identity(stage) == record["stage_id"]:
                    try:
                        shutil.rmtree(stage)
                    except OSError:
                        logging.error("Installation staging cleanup failed: %s", stage)
                else:
                    logging.error("Installation stage identity changed; replacement preserved")
        for path, expected_id in reversed(parents):
            try:
                check_parents(path.parent, ancestors)
                if path.exists() and not linked(path) and identity(path) == expected_id:
                    path.rmdir()
            except OSError as error:
                if error.errno not in (errno.ENOTEMPTY, errno.EEXIST):
                    logging.warning("Installation parent cleanup failed: %s", path)
            except ValueError:
                logging.warning("Installation parent changed; recovery path preserved: %s", path)


def install(target: Path, *, plugin: bool, force: bool) -> Path | None:
    return execute_plan(plan_install([target], plugin=plugin, force=force))[0]


def execute(argv=None) -> int:
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
        targets = [args.destination] if args.destination else []
        if not targets:
            base = Path.home() if args.scope == "user" else args.path.resolve()
            if not base.is_dir():
                raise ValueError("Installation base directory does not exist")
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
        plan = plan_install(targets, plugin=args.plugin, force=args.force)
        if args.dry_run:
            for item in plan["targets"]:
                print(f"Would install: {item['target']}")
        else:
            backups = execute_plan(plan)
            for item, backup in zip(plan["targets"], backups):
                print(f"Installed: {item['target']}")
                if backup:
                    print(f"Previous installation retained: {backup}")
        logging.info("Installer completed exit=0 targets=%d", len(targets))
        return 0
    except (OSError, ValueError) as error:
        logging.error("Installer failed error_type=%s exit=2", type(error).__name__)
        print(f"ERROR: {error}", file=sys.stderr)
        return 2


def main(argv=None) -> int:
    try:
        with operational_log("install"):
            return execute(argv)
    except LogConfigurationError as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
