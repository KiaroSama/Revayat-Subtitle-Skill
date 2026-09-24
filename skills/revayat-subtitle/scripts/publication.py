"""Complete-file, same-filesystem publication without replacing a race winner."""

from __future__ import annotations

import logging
import os
from pathlib import Path
import uuid
import sys


def rename_noreplace(source: Path, destination: Path) -> None:
    """Use each supported OS's exclusive rename, including for directories."""
    if os.name == "nt":
        source.rename(destination)
        return
    import ctypes
    library = ctypes.CDLL(None, use_errno=True)
    if sys.platform == "darwin":
        function = getattr(library, "renamex_np", None)
        arguments = (os.fsencode(source), os.fsencode(destination), 4)  # RENAME_EXCL
        types = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint]
    else:
        function = getattr(library, "renameat2", None)
        arguments = (-100, os.fsencode(source), -100, os.fsencode(destination), 1)
        types = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
    if function is None:
        raise OSError("This platform lacks exclusive directory rename; no unsafe fallback was used")
    function.argtypes, function.restype = types, ctypes.c_int
    if function(*arguments) != 0:
        code = ctypes.get_errno()
        raise OSError(code, "Exclusive rename failed", str(destination))


def publish_bytes(destination: Path, data: bytes) -> None:
    destination = destination.absolute()
    if os.path.lexists(destination):
        raise FileExistsError("Output already exists; use a new filename")
    destination.parent.mkdir(parents=True, exist_ok=True)
    # A normal file inherits the destination ACL; tempfile's private ACL would
    # otherwise travel to the final path on Windows.
    stage = destination.parent / (".publish-" + uuid.uuid4().hex + ".tmp")
    owned = None
    committed = False
    try:
        with stage.open("xb") as handle:
            info = os.fstat(handle.fileno())
            owned = (info.st_dev, info.st_ino)
            if handle.write(data) != len(data):
                raise OSError("Publication write was incomplete")
            handle.flush()
            os.fsync(handle.fileno())
        if stage.read_bytes() != data:
            raise OSError("Staged publication bytes changed")
        if os.name == "nt":
            # Windows rename refuses an occupied target, unlike POSIX rename.
            stage.rename(destination)
        else:
            os.link(stage, destination, follow_symlinks=False)
        committed = True
        logging.debug("Publication committed bytes=%d", len(data))
        if os.name == "posix":
            directory = os.open(destination.parent, os.O_RDONLY)
            try:
                os.fsync(directory)
            finally:
                os.close(directory)
    except BaseException:
        if not committed and owned is not None and os.path.lexists(destination):
            try:
                info = destination.lstat()
                committed = not destination.is_symlink() and (info.st_dev, info.st_ino) == owned
            except OSError:
                logging.error("Publication outcome could not be inspected; verify the destination before retrying")
        logging.error("Publication failed committed=%s; a committed file is complete", committed)
        raise
    finally:
        if owned is not None:
            try:
                info = stage.lstat()
                if not stage.is_symlink() and (info.st_dev, info.st_ino) == owned:
                    stage.unlink()
                else:
                    logging.warning("Publication staging identity changed; unrelated replacement was preserved")
            except FileNotFoundError:
                pass
            except OSError:
                # The commit is complete; cleanup cannot make its filename retryable.
                # Before commit an existing primary error must remain the reported cause.
                logging.warning("Publication staging cleanup failed committed=%s; recovery file=%s", committed, stage)
