"""Finite AI calibration records preserve catalogue semantics and false certification."""
import unittest

from check import ROOT
from runtime import read_json


class EvaluationContractTests(unittest.TestCase):
    def test_pilot_has_real_disclosed_roles_and_no_certification(self):
        pilot = read_json(ROOT / "evaluation/pilot.json")
        self.assertEqual(pilot["version"], 1)
        self.assertFalse(pilot["linguistic_quality_certified"])
        self.assertEqual(len(pilot["cases"]), 5)
        review = pilot["independent_review"]
        self.assertIn("AI", review["role"])
        self.assertIn("not independent human", review["limitations"].lower())
        self.assertEqual({case["id"] for case in pilot["cases"]}, {case["id"] for case in review["cases"]})
        outcomes = {case["id"]: case["outcome"] for case in review["cases"]}
        self.assertEqual(outcomes["participant_reversal"], "meaning_changed")
        self.assertEqual(outcomes["negative_request"], "meaning_changed")
        self.assertEqual(outcomes["missing_referent"], "needs_context")
        self.assertEqual(outcomes["faithful_unseen"], "needs_context")
        self.assertEqual(pilot["cases"][0]["author_observation"]["outcome"], "faithful")
        self.assertEqual(len(read_json(ROOT / "evaluation/cases.json")), 24)


if __name__ == "__main__":
    unittest.main()
