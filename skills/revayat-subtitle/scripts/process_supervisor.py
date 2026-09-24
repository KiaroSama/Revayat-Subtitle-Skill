"""Internal Windows launch gate; the owner assigns this process to a Job first."""

import json
import logging
from pathlib import Path
import subprocess
import sys

# -I -S prevents site hooks and caller-controlled paths before Job assignment.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from runtime import operational_log


def main():
    with operational_log("process-supervisor"):
        payload = sys.stdin.buffer.readline(65537)
        if not payload:
            logging.error("Supervisor gate closed before launch")
            return 2
        if len(payload) > 65536 or not payload.endswith(b"\n"):
            raise ValueError("Invalid supervisor launch gate")
        command = json.loads(payload.decode("utf-8"))
        if not isinstance(command, list) or not command or any(not isinstance(v, str) or "\x00" in v for v in command):
            raise ValueError("Invalid supervisor argv")
        # The target starts only after the parent's Job assignment and gate release.
        process = subprocess.Popen(command, stdin=subprocess.DEVNULL,
                                   creationflags=subprocess.CREATE_NO_WINDOW)
        code = process.wait()
        logging.log(logging.ERROR if code else logging.DEBUG, "Supervised target exited code=%d", code)
        return code


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError):
        print("ERROR: Supervised target could not start.", file=sys.stderr)
        sys.exit(2)
