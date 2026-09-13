"""UTF-8 files, bounded child processes, and per-run operational logging."""

from __future__ import annotations

import contextlib
import hashlib
import json
import logging
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path: Path, value) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    data = (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    fd, temporary = tempfile.mkstemp(prefix=".write-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as handle:
            handle.write(data)
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


def local_path(root: Path, relative: str) -> Path:
    path = (root / relative).resolve()
    if not path.is_relative_to(root.resolve()):
        raise ValueError("Working file escapes its workspace")
    return path


def run(command: list[str], *, cwd: Path | None = None, timeout: int = 45) -> bytes:
    options = ({"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP} if os.name == "nt"
               else {"start_new_session": True})
    process = subprocess.Popen(command, cwd=cwd, stdin=subprocess.DEVNULL,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, **options)
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except BaseException:
        if process.poll() is None:
            if os.name == "nt":
                subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                               stdin=subprocess.DEVNULL, capture_output=True, timeout=10, check=False)
            else:
                os.killpg(process.pid, signal.SIGKILL)
        process.communicate(timeout=10)
        raise
    if process.returncode:
        # Tool output may contain paths, but subtitle bodies are never logged.
        logging.error("External tool failed: exit=%s", process.returncode)
        diagnostic = stdout[-3000:] + stderr[-1800:]
        raise ValueError("External tool failed (exit %s): %s" %
                         (process.returncode, diagnostic.decode("utf-8", errors="replace")))
    return stdout


@contextlib.contextmanager
def operational_log(name: str):
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8")
    directory = Path(os.environ.get("REVAYAT_LOG_DIR", Path(__file__).resolve().parent.parent / "logs"))
    handler = None
    began = time.monotonic()
    try:
        directory.mkdir(parents=True, exist_ok=True)
        stamp = time.strftime("%Y-%m-%d_%H-%M-%S_UTC", time.gmtime())
        for number in range(1000):
            suffix = f"_{number}" if number else ""
            try:
                handler = logging.FileHandler(directory / f"{name}_{stamp}{suffix}.log", mode="x", encoding="utf-8")
                break
            except FileExistsError:
                continue
        if handler is None:
            raise OSError("Log filename collision limit reached")
    except OSError:
        print("WARNING: File logging unavailable; using stderr.", file=sys.stderr)
        handler = logging.StreamHandler(sys.stderr)
    formatter = logging.Formatter("[%(asctime)s UTC] [%(levelname)s] [%(name)s] %(message)s", "%Y-%m-%d %H:%M:%S")
    formatter.converter = time.gmtime
    handler.setFormatter(formatter)
    logger = logging.getLogger()
    old_level = logger.level
    logger.setLevel(logging.INFO)
    logger.addHandler(handler)
    try:
        logging.info("Started component=%s python=%s platform=%s", name, sys.version.split()[0], sys.platform)
        yield
    finally:
        logging.info("Finished elapsed_seconds=%.3f", time.monotonic() - began)
        logger.removeHandler(handler)
        handler.close()
        logger.setLevel(old_level)
