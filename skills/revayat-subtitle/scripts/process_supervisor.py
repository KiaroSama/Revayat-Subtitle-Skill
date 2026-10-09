"""Internal Windows launch gate; the owner assigns this process to a Job first."""

import json
import logging
from pathlib import Path
import subprocess
import sys

# -I -S prevents site hooks and caller-controlled paths before Job assignment.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from runtime import operational_log
from process_control import checked_environment


def main():
    with operational_log("process-supervisor"):
        payload = sys.stdin.buffer.readline(2 * 1024 * 1024 + 1)
        if not payload:
            logging.error("Supervisor gate closed before launch")
            return 2
        if len(payload) > 2 * 1024 * 1024 or not payload.endswith(b"\n"):
            raise ValueError("Invalid supervisor launch gate")
        launch = json.loads(payload.decode("utf-8"))
        if not isinstance(launch, dict) or set(launch) != {"command", "env"}:
            raise ValueError("Invalid supervisor launch payload")
        command = launch["command"]
        env = checked_environment(launch["env"])
        if not isinstance(command, list) or not command or any(not isinstance(v, str) or "\x00" in v for v in command):
            raise ValueError("Invalid supervisor argv")
        logging.debug("Supervisor launch gate validated")
        # The target starts only after the parent's Job assignment and gate release.
        logging.debug("Supervisor target launch starting")
        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, env=env,
                                   creationflags=subprocess.CREATE_NO_WINDOW)
        logging.debug("Supervisor target started pid=%d; waiting for exit", process.pid)
        code = process.wait()
        logging.debug("Supervisor target wait completed pid=%d code=%d", process.pid, code)
        logging.log(logging.ERROR if code else logging.DEBUG, "Supervised target exited code=%d", code)
        return code


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError):
        print("ERROR: Supervised target could not start.", file=sys.stderr)
        sys.exit(2)
