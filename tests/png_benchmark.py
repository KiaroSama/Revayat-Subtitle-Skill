"""Same-input baseline/current PNG cost comparison; run only in final CI."""
import json
from pathlib import Path
import statistics
import sys
import time
import types
import unittest

from check import ROOT, WorkspaceCase
from runtime import run
from test_png_profile import png
import png_validation

BASE = "cb3c94655527cfcca246a7e5d4af783de250ef96"


class PngBenchmarkTests(WorkspaceCase):
    def test_dimensions_and_same_frame_cost(self):
        source = run(["git", "-C", str(ROOT), "show", BASE + ":skills/revayat-subtitle/scripts/png_validation.py"], timeout=8, max_output=65536)
        baseline = types.ModuleType("baseline_png")
        exec(compile(source.decode("utf-8"), "baseline_png.py", "exec"), baseline.__dict__)
        results = []
        for filter_ in (0, 1, 2, 3, 4):
            # Bounded authored RGB rows, identical bytes/runtime for both implementations.
            width, height = 320, 180
            data = png(width, height, filter_=filter_, pixels=(bytes([filter_]) + b"\x17" * (width * 3)) * height)
            timings = {"baseline": [], "current": []}
            for name, decoder in (("baseline", baseline.decode_png), ("current", png_validation.decode_png)):
                for _ in range(3):
                    began = time.monotonic()
                    self.assertEqual(decoder(data), (width, height))
                    timings[name].append(time.monotonic() - began)
            item = {"filter": filter_, **{name: statistics.median(values) for name, values in timings.items()}}
            results.append(item)
            print("PNG cost " + json.dumps(item), flush=True)
        # Removal of discarded predictors must improve their aggregate on the same input.
        self.assertLess(sum(item["current"] for item in results[1:]), sum(item["baseline"] for item in results[1:]))


if __name__ == "__main__":
    unittest.main()
