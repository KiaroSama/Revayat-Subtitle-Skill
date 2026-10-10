"""Captured test children reuse the production owner, never a second subprocess runner."""
import math
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


def print_deadline_snapshot(error):
    """Display only finite owner state after run() has completed its cleanup."""
    try:
        snapshot = getattr(error, 'deadline_snapshot', None)
        schema = {'elapsed_seconds': float, 'idle_elapsed_seconds': float, 'reason': str,
                  'gate_released': bool, 'stdout_closed': bool, 'stderr_closed': bool,
                  'leader_exited': bool, 'stdout_consumed_bytes': int,
                  'stderr_consumed_bytes': int, 'idle_gap_latched': bool}
        if type(snapshot) is not dict or set(snapshot) != set(schema):
            return
        if any(type(snapshot[key]) is not kind for key, kind in schema.items()):
            return
        if snapshot['reason'] not in ('wall', 'idle'):
            return
        if any(not math.isfinite(snapshot[key]) or not 0 <= snapshot[key] <= 172800
               for key in ('elapsed_seconds', 'idle_elapsed_seconds')):
            return
        if any(not 0 <= snapshot[key] <= 256 * 1024 * 1024
               for key in ('stdout_consumed_bytes', 'stderr_consumed_bytes')):
            return
        print('Native deadline ' + ' '.join(f'{key}={snapshot[key]}' for key in schema), flush=True)
    except Exception:
        # Best-effort reporting never substitutes an error for the original timeout.
        return
