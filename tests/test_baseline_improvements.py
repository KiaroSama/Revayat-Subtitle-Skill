"""Final-CI intended baseline failures, not fabricated local red/green evidence."""
import importlib.util
from pathlib import Path
import sys
import unittest

from check import ROOT, WorkspaceCase
from runtime import run

BASE = "cb3c94655527cfcca246a7e5d4af783de250ef96"


class BaselineImprovementTests(WorkspaceCase):
    def test_baseline_fails_only_named_fingerprint_sync_and_comment_contracts(self):
        baseline = self.root / "baseline"
        baseline.mkdir()
        for name in ("runtime.py", "markup.py"):
            raw = run(["git", "-C", str(ROOT), "show", BASE + ":skills/revayat-subtitle/scripts/" + name], timeout=8, max_output=65536)
            (baseline / name).write_bytes(raw)
        code = (
            "import importlib.util,sys,unittest;from pathlib import Path;from unittest.mock import patch;"
            "sys.path.insert(0,sys.argv[1]);import runtime,markup\n"
            "root=Path(sys.argv[2]);a=root/'a';b=root/'b';a.write_bytes(b'original');b.write_bytes(b'replaced');"
            "original=Path.open\n"
            "def opened(path,*args,**kwargs): return original(b if path==a else path,*args,**kwargs)\n"
            "with patch.object(Path,'open',opened):\n"
            " result=runtime.file_fingerprint(a)\n"
            "assert result['sha256'] != __import__('hashlib').sha256(b'original').hexdigest(), 'baseline fingerprint changed unexpectedly'\n"
            "p=root/'state.json';p.write_bytes(b'{\"old\":true}\\n')\n"
            "with patch.object(runtime.os,'fsync',side_effect=OSError('authored')): runtime.write_json(p,{'new':True})\n"
            "assert runtime.read_json(p)=={'new':True}, 'baseline sync behavior changed unexpectedly'\n"
            "assert markup.uncomment('A<<!--note-->b>B','srt')=='A<b>B', 'baseline comment behavior changed unexpectedly'\n"
            "print('baseline-unmet: opened-identity, sync-before-ack, literal-comment-activation')"
        )
        raw = run([sys.executable, "-I", "-S", "-B", "-c", code, str(baseline), str(self.root)],
                  timeout=8, idle_timeout=5, max_output=65536)
        self.assertIn(b"baseline-unmet: opened-identity, sync-before-ack, literal-comment-activation", raw)


if __name__ == "__main__":
    unittest.main()
