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
sys.path.insert(0, str(Path(__file__).resolve().parent))
import install_state as state
from install_recovery import restore, recover
from runtime import write_json

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


def plan_install(targets: list[Path], *, plugin: bool, force: bool, recovery_dir: Path | None = None) -> dict:
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
    roots = []
    for item in planned:
        target = item["target"]
        if recovery_dir is None:
            if target.name != NAME or target.parent.name != "skills":
                raise ValueError("Custom installation needs explicit --recovery-dir outside skill/plugin discovery")
            root = target.parent.parent / "revayat-recovery"
        else:
            root = recovery_dir
        root = state.safe_path(root)
        if (root == REPO or REPO.is_relative_to(root) or any(root.is_relative_to(p) or p.is_relative_to(root) for p in protected)
                or any(root.is_relative_to(p["target"]) or p["target"].is_relative_to(root) for p in planned)
                or any(part in {"skills", "plugins"} for part in root.parts)):
            raise ValueError("Recovery root overlaps source or skill/plugin discovery")
        existing = root
        while not existing.exists():
            existing = existing.parent
        target_parent = target.parent
        while not target_parent.exists():
            target_parent = target_parent.parent
        if existing.stat().st_dev != target_parent.stat().st_dev:
            raise ValueError("Recovery and installation must be on the same filesystem; no copy-delete fallback")
        for parent in (root, *root.parents):
            ancestors.setdefault(parent, identity(parent) if os.path.lexists(parent) else None)
        roots.append(root)
    # All selected targets share one journal; the first standard agent root owns recovery.
    return {"targets": planned, "files": pairs, "ancestors": ancestors, "recovery": roots[0]}


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
    with state.ownership() as owner:
        return _execute_owned(plan, owner)


def _execute_owned(plan: dict, owner: Path) -> list[Path | None]:
    state.refuse_pending(owner, [item["target"] for item in plan["targets"]])
    expected = {}
    for source, relative in plan["files"]:
        fingerprint = file_fingerprint(source, state.MAX_FILE)
        expected[relative.as_posix()] = [fingerprint["bytes"], fingerprint["sha256"]]
    directories = sorted({parent.as_posix() for name in expected for parent in Path(name).parents if parent != Path(".")})
    new = {"files": expected, "directories": directories}
    state.validate_manifest(new)
    ancestors, parents = dict(plan["ancestors"]), []
    old = []
    for item in plan["targets"]:
        check_parents(item["target"].parent, ancestors)
        current = identity(item["target"]) if os.path.lexists(item["target"]) else None
        if current != item["previous"]:
            raise ValueError("Installation target changed after preflight")
        old.append(state.tree_manifest(item["target"]) if current is not None else None)
    recovery = state.safe_path(plan["recovery"])
    check_parents(recovery, ancestors)
    make_parents(recovery, parents, ancestors)
    check_parents(recovery, ancestors)
    state.safe_path(recovery)
    identifier = uuid.uuid4().hex
    transaction = recovery / ("transaction-" + identifier)
    transaction.mkdir()
    journal = transaction / "journal.json"
    records = [{"target": str(item["target"]), "stage": str(transaction / f"new-{index:04}"),
                "backup": str(transaction / f"old-{index:04}"), "disposal": str(transaction / f"dispose-{index:04}"),
                "previous_id": state.identity(item["target"]) if item["previous"] is not None else None,
                "stage_id": None, "old": old[index], "new": new, "phase": "staging"}
               for index, item in enumerate(plan["targets"])]
    data = {"version": 1, "id": identifier, "owner": str(state.safe_path(Path.home())),
            "recovery": str(recovery), "state": "staging", "records": records,
            "ancestors": {str(path): state.identity(path) for path in ancestors if os.path.lexists(path)}}
    for path in (transaction, *transaction.parents):
        data["ancestors"][str(path)] = state.identity(path)
    write_json(journal, data)
    state.register(owner, journal)
    try:
        for record in records:
            target, stage = Path(record["target"]), Path(record["stage"])
            make_parents(target.parent, parents, ancestors)
            for path in (target.parent, *target.parent.parents):
                data["ancestors"][str(path)] = state.identity(path)
            state.save_journal(owner, journal, data)
            stage.mkdir()
            record["stage_id"] = state.identity(stage)
            state.save_journal(owner, journal, data)
            for source, relative in plan["files"]:
                output = stage / relative
                output.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, output)
                with output.open("r+b") as handle:
                    os.fsync(handle.fileno())
            if not state.matches(stage, record["stage_id"], new):
                raise ValueError("Installation source changed during staging")
            record["phase"] = "staged"
            state.save_journal(owner, journal, data)
        data["state"] = "staged"
        state.save_journal(owner, journal, data)
        for record in records:
            target, stage, backup = (Path(record[key]) for key in ("target", "stage", "backup"))
            check_parents(target.parent, ancestors)
            if record["previous_id"] is not None:
                if not state.matches(target, record["previous_id"], record["old"]):
                    raise ValueError("Previous installation changed before backup")
                record["phase"] = "old-intent"
                state.save_journal(owner, journal, data)
                state.move(target, backup, rename_noreplace)
                record["phase"] = "old-moved"
                state.save_journal(owner, journal, data)
            elif os.path.lexists(target):
                raise ValueError("Installation target became occupied")
            record["phase"] = "new-intent"
            state.save_journal(owner, journal, data)
            check_parents(target.parent, ancestors)
            state.move(stage, target, rename_noreplace)
            if not state.matches(target, record["stage_id"], new):
                raise ValueError("Published installation bytes changed")
            record["phase"] = "published"
            state.save_journal(owner, journal, data)
        data["state"] = "committed"
        state.save_journal(owner, journal, data)
        state.unregister(owner, journal)
        logging.info("Installed targets=%d journal=%s", len(records), journal)
        return [Path(record["backup"]) if record["previous_id"] is not None else None for record in records]
    except BaseException as original:
        try:
            committed = state.load_journal(journal)["state"] == "committed"
            if not committed:
                restore(journal, owner, rename_noreplace, explicit=False)
        except (OSError, ValueError):
            raise ValueError("Installation rollback incomplete; preserve recovery paths: " + str(journal)
                             + " | backups=" + ", ".join(record["backup"] for record in records
                                                         if record["previous_id"] is not None)) from original
        raise
    finally:
        for record in records:
            stage = Path(record["stage"])
            try:
                state.safe_path(stage)
                if record["stage_id"] is not None and state.matches(stage, record["stage_id"], record["new"]):
                    shutil.rmtree(stage)
            except (OSError, ValueError):
                logging.error("Installation staging cleanup failed; outcome preserved; recovery path: %s", stage)
        for path, expected_id in reversed(parents):
            try:
                check_parents(path.parent, ancestors)
                if path.exists() and not linked(path) and identity(path) == expected_id:
                    path.rmdir()
            except (OSError, ValueError):
                logging.debug("Installation parent retained: %s", path)


def install(target: Path, *, plugin: bool, force: bool, recovery_dir: Path | None = None) -> Path | None:
    with state.ownership() as owner:
        return _execute_owned(plan_install([target], plugin=plugin, force=force, recovery_dir=recovery_dir), owner)[0]


def execute(argv=None) -> int:
    with state.ownership() as owner:
        return _execute_cli(argv, owner)


def _execute_cli(argv, owner) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--agent", choices=(*AGENTS, "all"), default="all")
    parser.add_argument("--scope", choices=("user", "project"), default="user")
    parser.add_argument("--path", type=Path, default=Path.cwd(), help="Existing project root for project scope")
    parser.add_argument("--destination", type=Path, help="Exact skill/plugin directory; overrides agent routing")
    parser.add_argument("--plugin", action="store_true", help="Copy the complete plugin to --destination")
    parser.add_argument("--force", action="store_true", help="Back up an existing installation before replacing it")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--recovery-dir", type=Path, help="Same-filesystem recovery root outside skill/plugin discovery")
    parser.add_argument("--recover", type=Path, help="Explicitly restore old trees from a noncommitted journal")
    args = parser.parse_args(argv)
    try:
        if sys.version_info < (3, 11):
            raise ValueError("Python 3.11+ is required; use an upstream-maintained interpreter")
        if args.recover:
            if not args.recovery_dir or args.destination or args.plugin or args.force or args.dry_run:
                raise ValueError("--recover needs --recovery-dir and cannot combine installation flags")
            journal = state.safe_path(args.recover, directory=False)
            data = state.load_journal(journal)
            if state.safe_path(args.recovery_dir) != Path(data["recovery"]):
                raise ValueError("--recovery-dir differs from the journal root")
            restore(journal, owner, rename_noreplace)
            print("Previous installation restored")
            return 0
        if args.destination and not args.recovery_dir:
            raise ValueError("Custom --destination requires explicit --recovery-dir")
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
        plan = plan_install(targets, plugin=args.plugin, force=args.force, recovery_dir=args.recovery_dir)
        if args.dry_run:
            for item in plan["targets"]:
                print(f"Would install: {item['target']}")
        else:
            backups = _execute_owned(plan, owner)
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
    except (LogConfigurationError, OSError, ValueError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
