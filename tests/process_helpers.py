"""Captured test children reuse the production owner, never a second subprocess runner."""
import subprocess

from runtime import run


def run_child(command, *, cwd=None, env=None, timeout=15, idle_timeout=None,
              max_output=1024 * 1024, text=False, encoding="utf-8"):
    result = run(command, cwd=cwd, env=env, timeout=timeout,
                 idle_timeout=min(timeout, 10) if idle_timeout is None else idle_timeout,
                 max_output=max_output, check=False)
    if text:
        return subprocess.CompletedProcess(command, result.returncode,
                                           result.stdout.decode(encoding), result.stderr.decode(encoding))
    return result
