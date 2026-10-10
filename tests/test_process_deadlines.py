"""Output-idle deadlines follow observed output, not delayed owner scheduling."""
import io
import queue
from pathlib import Path
import subprocess
import threading

REAL_THREAD = threading.Thread
import types
import unittest
from unittest.mock import patch

from check import ROOT
import process_control
from runtime import run

BASELINE = 'da586e73bdf34ba3dff71c2462ed373514c2c620'


class DeadlineScenario:
    """Finite OS adapters drive the real owner loop without starting a process."""

    def __init__(self, output_at=9.0, finished_at=9.2, resumed_at=10.5, output=b'progress',
                 stderr_output=b'', stderr_at=9.1):
        self.now = 0.0
        self.output_at = output_at
        self.finished_at = finished_at
        self.resumed_at = resumed_at
        self.output = output
        self.stderr_output, self.stderr_at = stderr_output, stderr_at
        self.producer_observations = []
        self.owner_observations = []
        self.threads = []
        self.cleanups = []
        self.process = self.Process(self)

    class Stream(io.BytesIO):
        def __init__(self, scenario, stdout):
            super().__init__()
            self.scenario, self.stdout, self.sent = scenario, stdout, False

        def read1(self, size):
            scenario = self.scenario
            output = scenario.output if self.stdout else scenario.stderr_output
            observed = scenario.output_at if self.stdout else scenario.stderr_at
            if not self.sent and output:
                self.sent = True
                scenario.now = max(scenario.now, observed)
                scenario.producer_observations.append(scenario.now)
                return output
            scenario.now = max(scenario.now, scenario.finished_at)
            return b''

    class Process:
        pid = 1234
        _handle = 1234

        def __init__(self, scenario):
            self.scenario = scenario
            self.stdout = DeadlineScenario.Stream(scenario, True)
            self.stderr = DeadlineScenario.Stream(scenario, False)
            self.stdin = io.BytesIO()

        def poll(self):
            return 0 if self.scenario.now >= self.scenario.finished_at else None

        def wait(self, timeout=None):
            self.scenario.cleanups.append('wait')
            return 0

    class Thread:
        def __init__(self, scenario, target, args=(), daemon=None):
            self.scenario, self.target, self.args = scenario, target, args
            scenario.threads.append(self)

        def start(self):
            self.target(*self.args)
            # Both reader adapters have finished; model only the owner's delay.
            if len(self.scenario.threads) >= 2:
                self.scenario.now = max(self.scenario.now, self.scenario.resumed_at)

        def join(self, timeout=None):
            self.scenario.cleanups.append('join')

        def is_alive(self):
            return False

    class Job:
        def __init__(self, scenario):
            self.scenario = scenario

        def assign(self, process):
            self.scenario.cleanups.append('assign')

        def terminate(self):
            self.scenario.cleanups.append('terminate')

        def close(self):
            self.scenario.cleanups.append('close')

    def invoke(self, module, *, max_output=1024):
        def thread(**kwargs):
            return self.Thread(self, **kwargs)

        with patch.object(module, 'time', types.SimpleNamespace(monotonic=lambda: self.now)), \
             patch.object(module.subprocess, 'Popen', return_value=self.process), \
             patch.object(module.threading, 'Thread', side_effect=thread), \
             patch.object(module, 'WindowsJob', side_effect=lambda: self.Job(self)), \
             patch.object(module.os, 'killpg', side_effect=lambda *args: self.cleanups.append('killpg'), create=True):
            return module.run(['authored-deadline-fixture'], timeout=20, idle_timeout=10,
                              max_output=max_output)

    def assert_cleaned(self, test):
        test.assertIn('wait', self.cleanups)
        test.assertEqual(self.cleanups.count('join'), len(self.threads))
        test.assertTrue(self.process.stdout.closed)
        test.assertTrue(self.process.stderr.closed)
        test.assertTrue(self.process.stdin.closed)
        test.assertTrue('killpg' in self.cleanups or 'terminate' in self.cleanups)


class TimedGetScenario(DeadlineScenario):
    """Two real drains observe output during the owner's timed queue waits."""

    def __init__(self):
        super().__init__(finished_at=19.97)
        self.now = 0.0
        self.release = [threading.Event(), threading.Event()]
        self.enqueued = [threading.Event(), threading.Event()]
        self.finish = threading.Event()
        self.ready = [threading.Event(), threading.Event()]
        self.first_consumed = False
        self.queue_instance = None
        self.process.stdout = self.TimedStream(self, True)
        self.process.stderr = self.TimedStream(self, False)

    class TimedStream(DeadlineScenario.Stream):
        def read1(self, size):
            scenario = self.scenario
            scenario.ready[0 if self.stdout else 1].set()
            if self.stdout and int(self.sent) < 2:
                index = int(self.sent)
                if not scenario.release[index].wait(1):
                    raise OSError('Authored output rendezvous timed out')
                scenario.now = max(scenario.now, (9.99, 19.95)[index])
                scenario.producer_observations.append(scenario.now)
                self.sent = index + 1
                return (b'first', b'second')[index]
            if not scenario.finish.wait(1):
                raise OSError('Authored EOF rendezvous timed out')
            return b''

    def make_queue(self, maxsize):
        scenario = self

        class ObservedQueue(queue.Queue):
            def put(self, item, *args, **kwargs):
                super().put(item, *args, **kwargs)
                if item[0] == 'stdout':
                    scenario.enqueued[0 if item[1] == b'first' else 1].set()

            def get(self, block=True, timeout=None):
                scenario.owner_observations.append(scenario.now)
                index = 1 if scenario.first_consumed else 0
                if not scenario.enqueued[index].is_set():
                    scenario.release[index].set()
                    if not scenario.enqueued[index].wait(1):
                        raise AssertionError('Actual producer did not enqueue output')
                item = super().get(block, timeout)
                if item[0] == 'stdout' and item[1] == b'first':
                    scenario.first_consumed = True
                    scenario.now = 19.91
                elif item[0] == 'stdout' and item[1] == b'second':
                    scenario.now = 19.97
                    scenario.finish.set()
                return item

        self.queue_instance = ObservedQueue(maxsize)
        return self.queue_instance

    def invoke(self, module, *, max_output=1024):
        def thread(**kwargs):
            actual = REAL_THREAD(**kwargs)
            self.threads.append(actual)
            original_start = actual.start

            def start():
                original_start()
                if len(self.threads) == 2:
                    for event in self.ready:
                        if not event.wait(1):
                            raise AssertionError('Actual output drains did not start')
                    self.now = 9.9
            actual.start = start
            return actual

        def killed(*args):
            self.cleanups.append('killpg')
            for event in (*self.release, self.finish):
                event.set()

        os_proxy = types.SimpleNamespace(name='posix', killpg=killed)
        try:
            with patch.object(module, 'time', types.SimpleNamespace(monotonic=lambda: self.now)), \
                 patch.object(module, 'os', os_proxy), \
                 patch.object(module, 'queue', types.SimpleNamespace(Queue=self.make_queue, Empty=queue.Empty, Full=queue.Full)), \
                 patch.object(module.subprocess, 'Popen', return_value=self.process), \
                 patch.object(module.threading, 'Thread', side_effect=thread):
                return module.run(['authored-timed-get-fixture'], timeout=20, idle_timeout=10,
                                  max_output=max_output)
        finally:
            for event in (*self.release, self.finish):
                event.set()
            for actual in self.threads:
                actual.join(2)
                if actual.is_alive():
                    raise AssertionError('Authored output drain survived cleanup')

    def assert_cleaned(self, test):
        test.assertIn('killpg', self.cleanups)
        test.assertIn('wait', self.cleanups)
        test.assertTrue(all(not thread.is_alive() for thread in self.threads))
        test.assertTrue(self.process.stdout.closed)
        test.assertTrue(self.process.stderr.closed)


class ProcessDeadlineTests(unittest.TestCase):
    def test_queued_output_and_completion_do_not_expire_idle_when_owner_resumes(self):
        scenario = DeadlineScenario()
        try:
            self.assertEqual(scenario.invoke(process_control), b'progress')
        finally:
            scenario.assert_cleaned(self)

    def test_pinned_baseline_expires_idle_for_the_same_timely_output(self):
        raw = run(['git', '-C', str(ROOT), 'show', BASELINE + ':skills/revayat-subtitle/scripts/process_control.py'],
                  timeout=8, idle_timeout=5, max_output=65536)
        baseline = types.ModuleType('authored_process_deadline_baseline')
        baseline.__file__ = str(Path(process_control.__file__))
        exec(compile(raw.decode('utf-8'), baseline.__file__, 'exec'), baseline.__dict__)
        for scenario, captured in ((DeadlineScenario(), b''), (TimedGetScenario(), b'first')):
            with self.subTest(scenario=type(scenario).__name__):
                try:
                    with self.assertRaises(subprocess.TimeoutExpired) as caught:
                        scenario.invoke(baseline)
                    self.assertEqual(caught.exception.timeout, 20)
                    self.assertEqual(caught.exception.output, captured)
                finally:
                    scenario.assert_cleaned(self)

    def test_output_observed_during_get_uses_actual_observation_time(self):
        scenario = TimedGetScenario()
        try:
            self.assertEqual(scenario.invoke(process_control), b'firstsecond')
            self.assertEqual(scenario.producer_observations, [9.99, 19.95])
            self.assertEqual(scenario.owner_observations[:2], [9.9, 19.91])
        finally:
            scenario.assert_cleaned(self)

    def test_both_capture_streams_share_timely_output_progress(self):
        scenario = DeadlineScenario(output_at=1, finished_at=1.1, resumed_at=15,
                                    output=b'early', stderr_output=b'later', stderr_at=9)
        try:
            self.assertEqual(scenario.invoke(process_control), b'early')
            self.assertEqual(scenario.producer_observations, [1, 9])
        finally:
            scenario.assert_cleaned(self)

    def test_gap_after_valid_output_is_not_revived_by_the_other_stream(self):
        scenario = DeadlineScenario(output_at=1, finished_at=1.1, resumed_at=11.3,
                                    output=b'early', stderr_output=b'late', stderr_at=11.1)
        try:
            with self.assertRaises(subprocess.TimeoutExpired):
                scenario.invoke(process_control)
            self.assertEqual(scenario.producer_observations, [1, 11.1])
        finally:
            scenario.assert_cleaned(self)

    def test_late_output_cannot_revive_a_real_idle_gap(self):
        for observed in (10.0, 10.01):
            with self.subTest(observed=observed):
                scenario = DeadlineScenario(output_at=observed, finished_at=10.1)
                try:
                    with self.assertRaises(subprocess.TimeoutExpired):
                        scenario.invoke(process_control)
                finally:
                    scenario.assert_cleaned(self)

    def test_eof_and_launch_gate_are_not_output_progress(self):
        scenario = DeadlineScenario(output=b'', finished_at=10.1)
        try:
            with self.assertRaises(subprocess.TimeoutExpired):
                scenario.invoke(process_control)
        finally:
            scenario.assert_cleaned(self)

    def test_absolute_wall_deadline_still_fails_with_timely_queued_output(self):
        scenario = DeadlineScenario(resumed_at=20.01)
        try:
            with self.assertRaises(subprocess.TimeoutExpired):
                scenario.invoke(process_control)
        finally:
            scenario.assert_cleaned(self)

    def test_pending_output_still_enforces_the_output_cap(self):
        scenario = DeadlineScenario(resumed_at=9.3)
        try:
            with self.assertRaisesRegex(ValueError, 'byte limit'):
                scenario.invoke(process_control, max_output=2)
        finally:
            scenario.assert_cleaned(self)


if __name__ == '__main__':
    unittest.main()
