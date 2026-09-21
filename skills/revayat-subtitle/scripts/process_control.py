"""Bounded cooperative tool supervision with independent process-tree ownership."""

from __future__ import annotations

import json
import logging
import math
import os
from pathlib import Path
import queue
import signal
import subprocess
import sys
import threading
import time


class ToolFailure(ValueError):
    """Bounded diagnostics are available to the owner, never interpolated into logs."""

    def __init__(self, code, stdout, stderr):
        super().__init__(f"External tool failed (exit {code})")
        self.returncode, self.stdout, self.stderr = code, stdout, stderr


class WindowsJob:
    def __init__(self):
        import ctypes
        from ctypes import wintypes
        self.ctypes = ctypes
        self.api = ctypes.WinDLL("kernel32", use_last_error=True)
        handle, dword = wintypes.HANDLE, wintypes.DWORD
        class Limits(ctypes.Structure):
            _fields_ = [("process_time", ctypes.c_longlong), ("job_time", ctypes.c_longlong),
                        ("flags", dword), ("minimum_working", ctypes.c_size_t),
                        ("maximum_working", ctypes.c_size_t), ("active_limit", dword),
                        ("affinity", ctypes.c_size_t), ("priority", dword), ("scheduling", dword)]
        class IO(ctypes.Structure):
            _fields_ = [(name, ctypes.c_ulonglong) for name in
                        ("reads", "writes", "other", "read_bytes", "write_bytes", "other_bytes")]
        class Extended(ctypes.Structure):
            _fields_ = [("basic", Limits), ("io", IO), ("process_memory", ctypes.c_size_t),
                        ("job_memory", ctypes.c_size_t), ("peak_process", ctypes.c_size_t),
                        ("peak_job", ctypes.c_size_t)]
        signatures = {
            "CreateJobObjectW": ([ctypes.c_void_p, wintypes.LPCWSTR], handle),
            "SetInformationJobObject": ([handle, ctypes.c_int, ctypes.c_void_p, dword], wintypes.BOOL),
            "AssignProcessToJobObject": ([handle, handle], wintypes.BOOL),
            "TerminateJobObject": ([handle, wintypes.UINT], wintypes.BOOL),
            "CloseHandle": ([handle], wintypes.BOOL),
            "QueryInformationJobObject": ([handle, ctypes.c_int, ctypes.c_void_p, dword, ctypes.c_void_p], wintypes.BOOL),
        }
        for name, (arguments, result) in signatures.items():
            function = getattr(self.api, name)
            function.argtypes, function.restype = arguments, result
        self.handle = self.api.CreateJobObjectW(None, None)
        if not self.handle:
            raise ctypes.WinError(ctypes.get_last_error())
        limits = Extended()
        limits.basic.flags = 0x2000  # JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE; no breakaway.
        if not self.api.SetInformationJobObject(self.handle, 9, ctypes.byref(limits), ctypes.sizeof(limits)):
            error = ctypes.WinError(ctypes.get_last_error())
            self.close()
            raise error

    def assign(self, process):
        # Popen retains the exact process handle: reopening its PID risks reuse.
        if not self.api.AssignProcessToJobObject(self.handle, int(process._handle)):
            raise self.ctypes.WinError(self.ctypes.get_last_error())

    def terminate(self):
        if not self.api.TerminateJobObject(self.handle, 1):
            raise self.ctypes.WinError(self.ctypes.get_last_error())
        # Accounting survives the leader, unlike PID-based descendant enumeration.
        buffer = self.ctypes.create_string_buffer(48)
        deadline = time.monotonic() + 5
        while True:
            if not self.api.QueryInformationJobObject(self.handle, 1, buffer, 48, None):
                raise self.ctypes.WinError(self.ctypes.get_last_error())
            active = int.from_bytes(buffer.raw[40:44], "little")
            if not active:
                return
            if time.monotonic() >= deadline:
                raise OSError("Owned Windows job did not terminate within the cleanup budget")
            threading.Event().wait(0.02)

    def close(self):
        if self.handle:
            handle, self.handle = self.handle, None
            if not self.api.CloseHandle(handle):
                raise self.ctypes.WinError(self.ctypes.get_last_error())


def _positive(value, name):
    if type(value) not in (int, float) or not 0 < value <= 86400 or not math.isfinite(value):
        raise ValueError(f"{name} must be a finite positive number")
    return float(value)


def run(command: list[str], *, cwd: Path | None = None, timeout: float = 45,
        idle_timeout: float | None = None, max_output: int = 16 * 1024 * 1024) -> bytes:
    timeout = _positive(timeout, "timeout")
    idle = _positive(timeout if idle_timeout is None else idle_timeout, "idle_timeout")
    if type(max_output) is not int or not 1 <= max_output <= 256 * 1024 * 1024:
        raise ValueError("max_output must be an integer from 1 to 268435456 bytes")
    if not isinstance(command, list) or not command or not command[0] or any(not isinstance(v, str) or "\x00" in v for v in command):
        raise ValueError("Tool command must be a nonempty argv list of strings")
    gate = (json.dumps(command, ensure_ascii=True) + "\n").encode("utf-8")
    if len(gate) > 65536:
        raise ValueError("Tool command exceeds the argument budget")
    job, process = None, None
    readers = []
    events = queue.Queue(maxsize=16)
    stopping = threading.Event()
    captured = {"stdout": bytearray(), "stderr": bytearray()}
    began = last_progress = time.monotonic()
    primary = None

    def emit(kind, data):
        while not stopping.is_set():
            try:
                events.put((kind, data), timeout=0.05)
                return
            except queue.Full:
                continue

    def drain(kind, stream):
        try:
            while not stopping.is_set():
                chunk = stream.read1(65536)
                if not chunk:
                    break
                emit(kind, chunk)
        except OSError:
            emit("capture_error", kind)
        finally:
            emit("eof", kind)

    def release_gate():
        try:
            process.stdin.write(gate)
            process.stdin.flush()
            process.stdin.close()
            emit("gate", None)
        except OSError:
            emit("gate_error", None)

    try:
        if os.name == "nt":
            job = WindowsJob()
            supervisor = Path(__file__).with_name("process_supervisor.py")
            process = subprocess.Popen([sys.executable, "-I", "-S", "-B", "-X", "utf8", str(supervisor)], cwd=cwd,
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                       creationflags=subprocess.CREATE_NO_WINDOW)
            try:
                job.assign(process)
            except BaseException:
                process.kill()  # Trusted supervisor has not received its launch gate.
                process.wait(timeout=5)
                raise
        else:
            process = subprocess.Popen(command, cwd=cwd, stdin=subprocess.DEVNULL,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        logging.debug("Tool started pid=%d wall=%s idle=%s output_limit=%d", process.pid, timeout, idle, max_output)
        for kind in ("stdout", "stderr"):
            reader = threading.Thread(target=drain, args=(kind, getattr(process, kind)), daemon=True)
            reader.start()
            readers.append(reader)
        gate_done = job is None
        if job is not None:
            sender = threading.Thread(target=release_gate, daemon=True)
            sender.start()
            readers.append(sender)
        finished = set()
        size = 0
        while len(finished) != 2 or process.poll() is None or not gate_done:
            now = time.monotonic()
            remaining = min(timeout - (now - began), idle - (now - last_progress))
            if remaining <= 0:
                # Never embed secret-bearing argv or captured private output in the exception.
                raise subprocess.TimeoutExpired("external tool", timeout,
                                                output=bytes(captured["stdout"]), stderr=bytes(captured["stderr"]))
            try:
                kind, data = events.get(timeout=min(0.1, remaining))
            except queue.Empty:
                continue
            if kind == "eof":
                finished.add(data)
            elif kind == "gate":
                gate_done = True
            elif kind in {"capture_error", "gate_error"}:
                raise OSError("External tool pipe or launch gate failed")
            else:
                size += len(data)
                if size > max_output:
                    raise ValueError("External tool output exceeded its byte limit")
                captured[kind].extend(data)
                last_progress = now
        code = process.wait(timeout=1)
        if code:
            raise ToolFailure(code, bytes(captured["stdout"]), bytes(captured["stderr"]))
        return bytes(captured["stdout"])
    except BaseException as error:
        primary = error
        raise
    finally:
        stopping.set()
        cleanup_error = None
        if process is not None:
            try:
                if job is not None:
                    job.terminate()
                else:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                process.wait(timeout=5)
            except (OSError, subprocess.TimeoutExpired) as error:
                cleanup_error = error
            finally:
                if job is not None:
                    try:
                        job.close()
                        process.wait(timeout=5)
                    except (OSError, subprocess.TimeoutExpired) as error:
                        cleanup_error = error
                deadline = time.monotonic() + 5
                for reader in readers:
                    reader.join(timeout=max(0, deadline - time.monotonic()))
                    if reader.is_alive():
                        cleanup_error = OSError("Tool capture thread did not finish after cleanup")
                # Buffered close can wait indefinitely for a live IO thread's lock.
                # Such a thread retains its stream; report failed cleanup, never block.
                if not any(reader.is_alive() for reader in readers):
                    for stream in (process.stdin, process.stdout, process.stderr):
                        try:
                            if stream is not None and not stream.closed:
                                stream.close()
                        except OSError as error:
                            cleanup_error = error
        elif job is not None:
            try:
                job.close()
            except OSError as error:
                cleanup_error = error
        if cleanup_error is not None:
            logging.error("Owned tool cleanup failed error_type=%s", type(cleanup_error).__name__)
            if primary is None:
                raise cleanup_error
        logging.debug("Tool finished elapsed=%.3f failed=%s", time.monotonic() - began, primary is not None)
