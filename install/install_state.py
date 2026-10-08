"""Same-account OS ownership and bounded, revision-checked installation recovery state."""
from __future__ import annotations

import contextlib
import errno
import logging
import os
from pathlib import Path
import re
import stat

from runtime import digest, file_fingerprint, read_json, read_limited, write_json
import json

MAX_ENTRIES = 10000
MAX_BYTES = 256 * 1024 * 1024
MAX_FILE = 16 * 1024 * 1024


def identity(path: Path):
    info = path.lstat()
    return [format(info.st_dev, "x"), format(info.st_ino, "x")]


def valid_identity(value):
    return (isinstance(value, list) and len(value) == 2
            and all(isinstance(part, str) and re.fullmatch(r"0|[1-9a-f][0-9a-f]{0,31}", part) for part in value))


def safe_path(path: Path, *, directory=True) -> Path:
    path = path.absolute()
    for item in (path, *path.parents):
        if os.path.lexists(item):
            info = item.lstat()
            if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 1024:
                raise ValueError("Installation state paths must not contain links or junctions")
            if item != path or directory:
                if not stat.S_ISDIR(info.st_mode):
                    raise ValueError("Installation state ancestor must be a directory")
    return path.resolve()


def tree_manifest(root: Path) -> dict:
    safe_path(root)
    pending, files, directories, count, total = [root], {}, [], 0, 0
    while pending:
        with os.scandir(pending.pop()) as entries:
            for entry in entries:
                count += 1
                if count > MAX_ENTRIES:
                    raise ValueError("Previous installation exceeds its complete-tree entry limit")
                path = Path(entry.path)
                info = entry.stat(follow_symlinks=False)
                name = path.relative_to(root).as_posix()
                if entry.is_symlink() or getattr(info, "st_file_attributes", 0) & 1024:
                    raise ValueError("Previous installation contains linked state; no partial backup was made")
                if stat.S_ISDIR(info.st_mode):
                    directories.append(name)
                    pending.append(path)
                elif stat.S_ISREG(info.st_mode):
                    if info.st_size > MAX_FILE or total + info.st_size > MAX_BYTES:
                        raise ValueError("Previous installation exceeds its complete-tree byte limit")
                    value = file_fingerprint(path, MAX_FILE)
                    files[name] = [value["bytes"], value["sha256"]]
                    total += value["bytes"]
                else:
                    raise ValueError("Previous installation contains a special file; no partial backup was made")
    return {"files": files, "directories": sorted(directories)}


def matches(path: Path, owned, manifest) -> bool:
    if not os.path.lexists(path):
        return False
    try:
        return identity(path) == owned and tree_manifest(path) == manifest
    except (OSError, ValueError):
        return False


def sync_directory(path: Path):
    if os.name == "posix":
        fd = os.open(path, os.O_RDONLY)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)


def move(source: Path, destination: Path, rename):
    rename(source, destination)
    sync_directory(source.parent)
    if destination.parent != source.parent:
        sync_directory(destination.parent)


@contextlib.contextmanager
def ownership():
    # ponytail: one same-account lock serializes installs; per-target locks only if throughput matters.
    home = safe_path(Path.home())
    root = safe_path(home / "revayat-install-state")
    root.mkdir(exist_ok=True)
    safe_path(root)
    path = root / "owner.lock"
    safe_path(path, directory=False)
    fd = os.open(path, os.O_RDWR | os.O_CREAT | getattr(os, "O_BINARY", 0), 0o600)
    held, primary = False, None
    try:
        opened = os.fstat(fd)
        if not stat.S_ISREG(opened.st_mode) or identity(path) != [format(opened.st_dev, "x"), format(opened.st_ino, "x")]:
            raise ValueError("Installation lock identity changed")
        os.lseek(fd, 0, os.SEEK_SET)
        try:
            if os.name == "nt":
                import msvcrt
                msvcrt.locking(fd, msvcrt.LK_NBLCK, 1)
            elif os.name == "posix":
                import fcntl
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            else:
                raise OSError("This platform lacks supported installation locking")
        except OSError as error:
            if error.errno in {errno.EACCES, errno.EAGAIN, errno.EDEADLK}:
                raise ValueError("Another same-account installation or recovery holds the OS lock") from None
            raise
        held = True
        if identity(path) != [format(opened.st_dev, "x"), format(opened.st_ino, "x")]:
            raise ValueError("Installation lock pathname changed")
        yield root
    except BaseException as error:
        primary = error
        raise
    finally:
        try:
            if held:
                os.lseek(fd, 0, os.SEEK_SET)
                if os.name == "nt":
                    import msvcrt
                    msvcrt.locking(fd, msvcrt.LK_UNLCK, 1)
                else:
                    import fcntl
                    fcntl.flock(fd, fcntl.LOCK_UN)
        except OSError:
            logging.error("Installation lock release failed; original outcome preserved")
            if primary is None:
                raise
        finally:
            try:
                os.close(fd)
            except OSError:
                logging.error("Installation lock close failed; original outcome preserved")
                if primary is None:
                    raise


def registry(root: Path) -> list[dict]:
    path = root / "pending.json"
    if not path.exists():
        return []
    safe_path(path, directory=False)
    value = read_json(path, MAX_FILE)
    if not isinstance(value, list) or len(value) > 1000:
        raise ValueError("Installation pending registry is invalid; preserve state")
    seen = set()
    for item in value:
        if (not isinstance(item, dict) or set(item) != {"path", "transaction_id", "digests", "active"}
                or type(item["path"]) is not str or item["path"] in seen
                or not valid_identity(item["transaction_id"]) or type(item["active"]) is not bool
                or not isinstance(item["digests"], list) or not 1 <= len(item["digests"]) <= 2
                or any(type(value) is not str or not re.fullmatch(r"[0-9a-f]{64}", value) for value in item["digests"])):
            raise ValueError("Installation pending registry is invalid; preserve state")
        seen.add(item["path"])
    return value


def register(root: Path, journal: Path):
    entries = registry(root)
    if any(item["path"] == str(journal) for item in entries):
        raise ValueError("Installation transaction is already registered")
    entries.append({"path": str(journal), "transaction_id": identity(journal.parent),
                    "digests": [digest(read_limited(journal, MAX_FILE))], "active": True})
    write_json(root / "pending.json", entries)


def registered(root: Path, journal: Path):
    entries = [item for item in registry(root) if item["path"] == str(journal)]
    if (len(entries) != 1 or identity(journal.parent) != entries[0]["transaction_id"]
            or digest(read_limited(journal, MAX_FILE)) not in entries[0]["digests"]):
        raise ValueError("Unregistered or changed installation transaction instructions; preserve state")
    return entries[0]


def save_journal(root: Path, journal: Path, data):
    registered(root, journal)
    # Registry independently binds both sides BEFORE the journal replacement.
    # A death/after-effect error admits only these exact legitimate byte revisions.
    raw = (json.dumps(data, ensure_ascii=False, indent=2, allow_nan=False) + "\n").encode("utf-8")
    if len(raw) > MAX_FILE:
        raise ValueError("Installation journal exceeds its byte limit")
    old = digest(read_limited(journal, MAX_FILE))
    new = digest(raw)
    entries = registry(root)
    current = next(item for item in entries if item["path"] == str(journal))
    current["digests"] = list(dict.fromkeys([old, new]))
    write_json(root / "pending.json", entries)
    try:
        write_json(journal, data)
    finally:
        # Keep both valid revisions on uncertainty; no fabricated phase acknowledgement.
        try:
            observed = digest(read_limited(journal, MAX_FILE))
            if observed in current["digests"]:
                current["digests"] = [observed]
                write_json(root / "pending.json", entries)
        except (OSError, ValueError):
            logging.warning("Journal binding reconciliation incomplete; both intended revisions retained")


def unregister(root: Path, journal: Path):
    registered(root, journal)
    entries = registry(root)
    next(item for item in entries if item["path"] == str(journal))["active"] = False
    write_json(root / "pending.json", entries)


def refuse_pending(root: Path, targets: list[Path]):
    for entry in registry(root):
        if not entry["active"]:
            continue
        path = Path(entry["path"])
        registered(root, path)
        data = load_journal(path)
        if data["state"] in {"committed", "restored"}:
            continue
        previous = [Path(record["target"]) for record in data["records"]]
        if any(a.is_relative_to(b) or b.is_relative_to(a) for a in targets for b in previous):
            raise ValueError(f"Interrupted installation needs explicit --recover: {path}")


def validate_manifest(value):
    if not isinstance(value, dict) or set(value) != {"files", "directories"}:
        raise ValueError("Invalid installation tree manifest")
    files, directories = value["files"], value["directories"]
    if not isinstance(files, dict) or not isinstance(directories, list) or len(files) + len(directories) > MAX_ENTRIES:
        raise ValueError("Installation tree manifest exceeds its entry limit")
    total = 0
    for name in [*files, *directories]:
        if (not isinstance(name, str) or not name or "\\" in name or ":" in name
                or name.startswith("/") or any(part in {"", ".", ".."} for part in name.split("/"))):
            raise ValueError("Invalid installation manifest relative path")
    if len(directories) != len(set(directories)) or set(files) & set(directories):
        raise ValueError("Duplicate installation manifest paths")
    for size_hash in files.values():
        if (not isinstance(size_hash, list) or len(size_hash) != 2 or type(size_hash[0]) is not int
                or not 0 <= size_hash[0] <= MAX_FILE or not isinstance(size_hash[1], str)
                or not re.fullmatch(r"[0-9a-f]{64}", size_hash[1])):
            raise ValueError("Invalid installation manifest fingerprint")
        total += size_hash[0]
    if total > MAX_BYTES:
        raise ValueError("Installation tree manifest exceeds its byte limit")


def load_journal(path: Path):
    path = safe_path(path, directory=False)
    data = read_json(path, MAX_FILE)
    if (not isinstance(data, dict) or set(data) != {"version", "id", "owner", "recovery", "state", "records", "ancestors"}
            or type(data["version"]) is not int or data["version"] != 1
            or not isinstance(data["id"], str) or not re.fullmatch(r"[0-9a-f]{32}", data["id"])
            or type(data["state"]) is not str or data["state"] not in {"staging", "staged", "restoring", "restored", "committed"}
            or data["owner"] != str(safe_path(Path.home()))):
        raise ValueError("Unknown or foreign installation journal; preserve state")
    if type(data["recovery"]) is not str or not data["recovery"] or "\x00" in data["recovery"]:
        raise ValueError("Invalid installation recovery path")
    recovery = safe_path(Path(data["recovery"]))
    if path != recovery / ("transaction-" + data["id"]) / "journal.json":
        raise ValueError("Installation journal is outside its declared recovery root")
    records = data["records"]
    if not isinstance(records, list) or not 1 <= len(records) <= 1000:
        raise ValueError("Installation journal target limit is invalid")
    targets = []
    for index, record in enumerate(records):
        if not isinstance(record, dict) or set(record) != {"target", "stage", "backup", "disposal", "previous_id", "stage_id", "old", "new", "phase"}:
            raise ValueError("Installation journal record fields are invalid")
        if any(type(record[field]) is not str or not record[field] or "\x00" in record[field]
               for field in ("target", "stage", "backup", "disposal")):
            raise ValueError("Invalid installation journal path field")
        target = safe_path(Path(record["target"]))
        if str(target) != record["target"] or any(target.is_relative_to(old) or old.is_relative_to(target) for old in targets):
            raise ValueError("Installation journal targets are noncanonical or overlap")
        targets.append(target)
        if (record["stage"] != str(path.parent / f"new-{index:04}") or record["backup"] != str(path.parent / f"old-{index:04}")
                or record["disposal"] != str(path.parent / f"dispose-{index:04}")):
            raise ValueError("Installation journal stage/backup paths are invalid")
        if record["previous_id"] is not None and not valid_identity(record["previous_id"]):
            raise ValueError("Invalid previous installation identity")
        if record["stage_id"] is not None and not valid_identity(record["stage_id"]):
            raise ValueError("Invalid staged installation identity")
        if type(record["phase"]) is not str or record["phase"] not in {"staging", "staged", "old-intent", "old-moved", "new-intent", "published", "restore-intent", "disposed", "restored"}:
            raise ValueError("Invalid installation journal phase")
        if record["old"] is not None:
            validate_manifest(record["old"])
        validate_manifest(record["new"])
        if (record["old"] is None) != (record["previous_id"] is None):
            raise ValueError("Previous installation identity/manifest mismatch")
    if not isinstance(data["ancestors"], dict) or len(data["ancestors"]) > 10000:
        raise ValueError("Invalid installation ancestor inventory")
    for name, expected in data["ancestors"].items():
        if not isinstance(name, str) or not valid_identity(expected) or str(safe_path(Path(name))) != name or identity(Path(name)) != expected:
            raise ValueError("Installation recovery ancestor changed; preserve state")
    return data
