"""Recognize known subtitle wordings; unknown translations need editorial review."""

from __future__ import annotations

import argparse
import json
import logging
from pathlib import Path
import sys
import unicodedata

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "skills" / "revayat-subtitle" / "scripts"))
from runtime import operational_log, read_json


def score(answers: dict) -> dict:
    cases = read_json(Path(__file__).with_name("cases.json"))
    expected = {case["id"] for case in cases}
    if not isinstance(answers, dict) or set(answers) - expected:
        raise ValueError("Answers must map known case IDs to strings")
    if any(not isinstance(answer, str) for answer in answers.values()):
        raise ValueError("Every answer must be a string")
    results = {}
    for case in cases:
        answer = unicodedata.normalize("NFC", answers.get(case["id"], "")).strip()
        known = {unicodedata.normalize("NFC", value).strip() for value in case["accepted"]}
        results[case["id"]] = "missing" if not answer else "known_wording" if answer in known else "needs_review"
    return {"results": results, "counts": {state: list(results.values()).count(state)
            for state in ("known_wording", "needs_review", "missing")},
            "linguistic_quality_certified": False}


def main(argv=None) -> int:
    with operational_log("evaluation"):
        parser = argparse.ArgumentParser(description=__doc__)
        parser.add_argument("--answers", required=True, type=Path)
        args = parser.parse_args(argv)
        try:
            result = score(read_json(args.answers))
            print(json.dumps(result, ensure_ascii=False, indent=2))
            code = int(bool(result["counts"]["needs_review"] or result["counts"]["missing"]))
            logging.info("Evaluation completed exit=%d counts=%s", code, result["counts"])
            return code
        except (OSError, ValueError) as error:
            logging.error("Evaluation failed error_type=%s exit=2", type(error).__name__)
            print(f"ERROR: {error}", file=sys.stderr)
            return 2


if __name__ == "__main__":
    sys.exit(main())
