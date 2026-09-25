"""UTF-8 files, bounded child processes, and per-run operational logging."""

from __future__ import annotations

import contextlib
import hashlib
import json
import logging
import math
import re
import os
from pathlib import Path
import signal
import stat
import shutil
import subprocess
import sys
import tempfile
import time
import uuid


MAX_JSON_BYTES = 64 * 1024 * 1024


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_limited(path: Path, maximum: int) -> bytes:
    if type(maximum) is not int or maximum < 0:
        raise ValueError("File byte limit must be a nonnegative integer")
    if not stat.S_ISREG(path.stat().st_mode):
        raise ValueError(f"{path.name}: expected a regular artifact file")
    with path.open("rb") as handle:
        if not stat.S_ISREG(os.fstat(handle.fileno()).st_mode):
            raise ValueError(f"{path.name}: artifact type changed while opening")
        data = handle.read(maximum + 1)
    if len(data) > maximum:
        raise ValueError(f"{path.name}: file exceeds the supported byte limit")
    return data


def file_fingerprint(path: Path, maximum: int = 16 * 1024**3) -> dict:
    before = path.stat()
    if not path.is_file() or before.st_size > maximum:
        raise ValueError("Fingerprint input exceeds the supported file-size limit")
    hasher, total = hashlib.sha256(), 0
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            total += len(chunk)
            if total > maximum:
                raise ValueError("Fingerprint input grew beyond its size limit")
            hasher.update(chunk)
    after = path.stat()
    if (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) != (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns):
        raise ValueError("Fingerprint input changed while reading")
    return {"name": path.name, "bytes": total, "sha256": hasher.hexdigest()}


def read_json(path: Path, maximum: int = 64 * 1024 * 1024):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError("duplicate object key")
            result[key] = value
        return result

    def nonfinite(_):
        raise ValueError("non-finite JSON number")

    def finite_float(value):
        if len(value) > 64:
            raise ValueError("JSON number is too long")
        result = float(value)
        if not math.isfinite(result):
            raise ValueError("non-finite JSON number")
        return result

    def bounded_integer(value):
        if len(value) > 21:
            raise ValueError("JSON integer is too long")
        return int(value)

    if type(maximum) is not int or maximum < 0:
        raise ValueError("JSON byte limit must be a nonnegative integer")
    maximum = min(maximum, MAX_JSON_BYTES)
    if path.stat().st_size > maximum:
        raise ValueError(f"{path.name}: JSON exceeds its byte limit")
    try:
        return json.loads(read_limited(path, maximum).decode("utf-8"), object_pairs_hook=pairs,
                          parse_constant=nonfinite, parse_float=finite_float, parse_int=bounded_integer)
    except (ValueError, RecursionError) as error:
        raise ValueError(f"{path.name}: invalid JSON ({type(error).__name__})") from None


def write_json(path: Path, value) -> None:
    data = (json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + "\n").encode("utf-8")
    if len(data) > MAX_JSON_BYTES:
        raise ValueError(f"{path.name}: JSON exceeds its byte limit")
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=".write-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as handle:
            if handle.write(data) != len(data):
                raise OSError("JSON write was incomplete")
        os.replace(temporary, path)
    finally:
        try:
            Path(temporary).unlink(missing_ok=True)
        except OSError:
            logging.warning("JSON staging cleanup failed; original operation result is preserved")


def local_path(root: Path, relative: str) -> Path:
    if (not isinstance(relative, str) or not relative or "\\" in relative or ":" in relative
            or relative.startswith("/") or any(part in {"", ".", ".."} for part in relative.split("/"))):
        raise ValueError("Working path must be a canonical relative path")
    lexical = root / relative
    for item in (lexical, *lexical.parents):
        if item == root.parent:
            break
        if item.is_symlink() or (item.exists() and getattr(item.lstat(), "st_file_attributes", 0) & 1024):
            raise ValueError("Linked working artifacts are not accepted")
    path = (root / relative).resolve()
    if not path.is_relative_to(root.resolve()):
        raise ValueError("Working file escapes its workspace")
    return path


def output_directory(parent: Path, prefix: str) -> Path:
    # Windows tempfile directories use an owner-only DACL. Published output must
    # inherit its chosen destination's permissions, including a desktop viewer.
    path = parent.resolve() / (prefix + uuid.uuid4().hex)
    path.mkdir()
    return path


@contextlib.contextmanager
def staging_directory(parent: Path, prefix: str):
    path = output_directory(parent, prefix)
    try:
        yield path
    finally:
        if path.exists():
            if path.resolve().parent != parent.resolve():
                raise ValueError("Staging cleanup escapes its parent directory")
            shutil.rmtree(path)


def run(command: list[str], *, cwd: Path | None = None, timeout: float = 45,
        idle_timeout: float | None = None, max_output: int = 16 * 1024 * 1024) -> bytes:
    from process_control import ToolFailure, run as run_owned
    try:
        return run_owned(command, cwd=cwd, timeout=timeout, idle_timeout=idle_timeout, max_output=max_output)
    except ToolFailure as error:
        script = None
        if Path(command[0]).resolve() == Path(sys.executable).resolve():
            arguments = iter(command[1:])
            for argument in arguments:
                if argument in {"-c", "-m"}:
                    break
                if argument in {"-X", "-W"}:
                    next(arguments, None)
                elif not argument.startswith("-"):
                    script = Path(argument).resolve()
                    break
        if script == Path(__file__).with_name("revayat-subtitle.py").resolve():
            lines = error.stderr.decode("utf-8", errors="replace").splitlines()
            detail = next((line[7:][:1000] for line in lines if line.startswith("ERROR: ")), "")
            if detail:
                error.args = (f"{error}: {detail}",)
        evaluator = Path(__file__).resolve().parents[3] / "evaluation/score.py"
        if script == evaluator:
            try:
                counts = json.loads(error.stdout)["counts"]
                if all(type(counts.get(key)) is int for key in ("needs_review", "missing")):
                    error.args = (f"{error}: needs_review={counts['needs_review']} missing={counts['missing']}",)
            except (ValueError, KeyError, TypeError):
                pass
        raise


class LogConfigurationError(ValueError):
    pass


def log_fallback(component: str, level: str, message: str):
    stamp = time.strftime("%Y-%m-%d %H:%M:%S", time.gmtime())
    print(f"[{stamp} UTC] [{level}] [{component}] {message}", file=sys.stderr)


class ResilientFileHandler(logging.FileHandler):
    def __init__(self, path):
        self.failed = False
        self.fallback = logging.StreamHandler(sys.stderr)
        super().__init__(path, mode="x", encoding="utf-8")

    def setFormatter(self, formatter):
        super().setFormatter(formatter)
        self.fallback.setFormatter(formatter)

    def handleError(self, record):
        if not self.failed:
            log_fallback("logging", "WARNING", "File logging failed; subsequent entries use stderr.")
            self.failed = True
        self.fallback.emit(record)

    def emit(self, record):
        if self.failed:
            self.fallback.emit(record)
        else:
            try:
                super().emit(record)
            except (OSError, ValueError):
                self.handleError(record)


@contextlib.contextmanager
def operational_log(name: str):
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8")
    level_name = os.environ.get("REVAYAT_LOG_LEVEL", "INFO").strip().upper()
    levels = {key: getattr(logging, key) for key in ("DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL")}
    if level_name not in levels or not re.fullmatch(r"[A-Za-z0-9_-]{1,80}", name):
        raise LogConfigurationError("Invalid logging configuration; REVAYAT_LOG_LEVEL must be DEBUG, INFO, WARNING, ERROR or CRITICAL")
    directory = Path(os.environ.get("REVAYAT_LOG_DIR", Path(__file__).resolve().parent.parent / "logs"))
    handler = None
    began = time.monotonic()
    try:
        directory.mkdir(parents=True, exist_ok=True)
        stamp = time.strftime("%Y-%m-%d_%H-%M-%S_UTC", time.gmtime())
        for number in range(1000):
            suffix = f"_{number}" if number else ""
            try:
                handler = ResilientFileHandler(directory / f"{name}_{stamp}{suffix}.log")
                break
            except FileExistsError:
                continue
        if handler is None:
            raise OSError("Log filename collision limit reached")
    except (OSError, ValueError):
        log_fallback(name, "WARNING", "File logging unavailable; using stderr.")
        handler = logging.StreamHandler(sys.stderr)

    class ContextFilter(logging.Filter):
        had_error = False
        def filter(self, record):
            record.component = name if record.name == "root" else record.name
            self.had_error = self.had_error or record.levelno >= logging.ERROR
            return True

    observer = ContextFilter()
    handler.addFilter(observer)
    formatter = logging.Formatter("[%(asctime)s UTC] [%(levelname)s] [%(component)s] %(message)s", "%Y-%m-%d %H:%M:%S")
    formatter.converter = time.gmtime
    handler.setFormatter(formatter)
    logger = logging.getLogger()
    old_level = logger.level
    logger.setLevel(levels[level_name])
    logger.addHandler(handler)
    failed = False
    try:
        logging.info("Started python=%s platform=%s", sys.version.split()[0], sys.platform)
        yield handler
    except BaseException as error:
        failed = not isinstance(error, SystemExit) or bool(error.code)
        if failed:
            logging.error("Operation failed error_type=%s", type(error).__name__)
        raise
    finally:
        failed = failed or observer.had_error
        logger.log(logging.ERROR if failed else logging.INFO, "Finished status=%s elapsed_seconds=%.3f",
                   "failed" if failed else "complete", time.monotonic() - began)
        logger.removeHandler(handler)
        try:
            handler.close()
        except (OSError, ValueError):
            log_fallback(name, "WARNING", "Log close failed; operation outcome was recorded above.")
        logger.setLevel(old_level)
