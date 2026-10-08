"""Real Windows native candidate fallback honors caller preferences and exact status."""
import json
import os
from pathlib import Path
import shutil
import sys
import unittest

from check import ROOT, WorkspaceCase
from process_helpers import run_child


@unittest.skipUnless(os.name == "nt", "Windows PowerShell native preference contract")
class BootstrapPreferenceTests(WorkspaceCase):
    def test_first_bad_native_candidate_then_good_preserves_argv_and_status(self):
        fixture = self.root / "bootstrap"
        fixture.mkdir()
        shutil.copyfile(ROOT / "install/install.ps1", fixture / "install.ps1")
        output = self.root / "forwarded.json"
        (fixture / "install.py").write_text("import json,sys;from pathlib import Path;Path(sys.argv[1]).write_text(json.dumps(sys.argv[2:],ensure_ascii=False),encoding='utf-8');sys.exit(7)", encoding="utf-8")
        bad = self.root / "bad native"
        good = self.root / "good native"
        bad.mkdir()
        good.mkdir()
        # Real Python binary + its runtime tree, not a CMD quoting approximation.
        base = Path(sys.executable).parent
        for directory, executable in ((bad, "python.exe"), (good, "python3.exe")):
            shutil.copyfile(sys.executable, directory / executable)
            for dll in base.glob("*.dll"):
                shutil.copyfile(dll, directory / dll.name)
        site = self.root / "site"
        site.mkdir()
        (site / "sitecustomize.py").write_text("import sys\nif sys.executable.lower().endswith('python.exe'):\n sys.version_info=(3,10,0,'final',0)\n", encoding="utf-8")
        caller = self.root / "caller.ps1"
        caller.write_text("param($Launcher,$Output)\n$ErrorActionPreference='Stop'\n$PSNativeCommandUseErrorActionPreference=$true\n& $Launcher $Output 'فارسی spaces' 'literal&value'\n", encoding="utf-8")
        env = {**os.environ, "PATH": str(bad) + os.pathsep + str(good) + os.pathsep + os.environ.get("PATH", ""),
               "PYTHONHOME": str(Path(sys.base_prefix)), "PYTHONPATH": str(site), "REVAYAT_LOG_DIR": str(self.root / "logs")}
        probe = "import sys;sys.exit(0 if sys.version_info >= (3,11) else 1)"
        self.assertEqual(run_child([str(bad / 'python.exe'), '-B', '-c', probe], env=env, timeout=8).returncode, 1)
        self.assertEqual(run_child([str(good / 'python3.exe'), '-B', '-c', probe], env=env, timeout=8).returncode, 0)
        shells = [shutil.which("pwsh"), shutil.which("powershell.exe")]
        for shell in dict.fromkeys(shell for shell in shells if shell):
            with self.subTest(shell=Path(shell).name):
                output.unlink(missing_ok=True)
                result = run_child([shell, "-NoProfile", "-NonInteractive", "-File", str(caller),
                                    str(fixture / "install.ps1"), str(output)], env=env, cwd=self.root, timeout=20)
                self.assertEqual(result.returncode, 7, result.stderr.decode("utf-8"))
                self.assertEqual(json.loads(output.read_text(encoding="utf-8")), ["فارسی spaces", "literal&value"])
        logs = [path.read_text(encoding="utf-8") for path in (self.root / "logs").glob("install-bootstrap_*.log")]
        self.assertTrue(logs)
        self.assertTrue(all("exit=7" in text for text in logs))
        self.assertTrue(all("literal&value" not in text for text in logs))


    def test_actual_launcher_function_scopes_native_preference_and_caller_status(self):
        source = (ROOT / 'install/install.ps1').read_text(encoding='utf-8')
        # Exercise native preference scopes in the actual body. Only path/terminal
        # exit adapters change so a function can inspect caller state after return.
        body = source.replace('$PSScriptRoot', '$FixtureRoot').replace('exit $result', 'return $result').replace('exit 2', 'return 2')
        fixture = self.root / 'scope fixture'
        fixture.mkdir()
        (fixture / 'install.py').write_text('import sys;sys.exit(7)', encoding='utf-8')
        caller = self.root / 'scope.ps1'
        caller.write_text('param($FixtureRoot)\nfunction Invoke-ActualLauncher {\n' + body
                          + '\n}\nforeach($choice in @($true,$false)) {\n'
                          + '$PSNativeCommandUseErrorActionPreference=$choice;$LASTEXITCODE=91;'
                          + 'Invoke-ActualLauncher;'
                          + 'if($PSNativeCommandUseErrorActionPreference -ne $choice -or $LASTEXITCODE -ne 91){throw "Caller state changed"}\n'
                          + '}\n', encoding='utf-8')
        for shell in dict.fromkeys(filter(None, [shutil.which('pwsh'), shutil.which('powershell.exe')])):
            result = run_child([shell, '-NoProfile', '-NonInteractive', '-File', str(caller), str(fixture)], timeout=20)
            self.assertEqual(result.returncode, 0, result.stderr.decode('utf-8'))


if __name__ == "__main__":
    unittest.main()
