"""Real Windows native candidate fallback honors caller preferences and exact status."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import unittest
from unittest.mock import patch

from check import ROOT, WorkspaceCase
from process_helpers import run_child
from runtime import operational_log, read_limited


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
        # Windows PowerShell 5.1 requires a UTF-8 BOM for non-ASCII script literals.
        caller.write_text("param($Launcher,$Output)\n$ErrorActionPreference='Stop'\n$PSNativeCommandUseErrorActionPreference=$true\n& $Launcher $Output 'فارسی spaces' 'literal&value'\nexit $LASTEXITCODE\n", encoding="utf-8-sig")
        self.assertTrue(caller.read_bytes().startswith(b"\xef\xbb\xbf"))
        self.assertIn("فارسی spaces", caller.read_text(encoding="utf-8-sig"))
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
        caller, fixture = self.write_scope_fixture()
        for shell in dict.fromkeys(filter(None, [shutil.which('pwsh'), shutil.which('powershell.exe')])):
            with self.subTest(shell=Path(shell).name):
                self.run_scope_fixture(shell, caller, fixture, 'original', None, {})

    def test_native_scope_fresh_and_reused_homes_fixed_sample(self):
        # Cold is first use of this authored home, not an OS/runtime cache flush.
        planned, attempted, passed, failed = 12, 0, 0, 0
        first_failure = 'none'
        try:
            shells = [shutil.which('pwsh'), shutil.which('powershell.exe')]
            for name, shell in zip(('pwsh', 'powershell.exe'), shells):
                if shell is None:
                    first_failure = f'prerequisite:{name}'
                    self.fail(f'Fixed native sample requires {name}')
            caller, fixture = self.write_scope_fixture()
            for pair in range(3):
                for shell in shells:
                    home = self.root / f'fresh home فارسی {Path(shell).stem}-{pair}'
                    home.mkdir()
                    env = {'HOME': str(home), 'USERPROFILE': str(home)}
                    for phase in ('fresh', 'reused'):
                        label = f'{Path(shell).stem}-{pair}-{phase}'
                        observation = {'phase': 'not-observed'}
                        attempted += 1
                        try:
                            self.run_scope_fixture(shell, caller, fixture, label, env, observation)
                        except BaseException as error:
                            failed += 1
                            first_failure = f'{label}:{type(error).__name__}:{observation["phase"]}'
                            raise
                        passed += 1
        finally:
            print(f'Native fixed sample planned={planned} attempted={attempted} passed={passed} '
                  f'failed={failed} not_run={planned-attempted} first_failure={first_failure}', flush=True)
        self.assertEqual((attempted, passed, failed), (planned, planned, 0))

    def test_native_scope_silent_and_marked_fixed_comparison(self):
        planned, attempted, passed, failed = 4, 0, 0, 0
        first_failure = 'none'
        try:
            shell = shutil.which('powershell.exe')
            if shell is None:
                first_failure = 'prerequisite:powershell.exe'
                self.fail('Fixed silent comparison requires powershell.exe')
            caller, fixture = self.write_scope_fixture()
            silent = caller.with_name('scope-silent.ps1')
            silent.write_text(self.scope_caller_text(False), encoding='utf-8')
            for index, marked in enumerate((False, True, True, False)):
                home = self.root / f'comparison home فارسی {index}'
                home.mkdir()
                env = {'HOME': str(home), 'USERPROFILE': str(home)}
                label = f'comparison-{index}-' + ('marked' if marked else 'silent')
                observation = {'phase': 'not-observed'}
                attempted += 1
                try:
                    self.run_scope_fixture(shell, caller if marked else silent, fixture, label, env, observation)
                except BaseException as error:
                    failed += 1
                    first_failure = f'{label}:{type(error).__name__}:{observation["phase"]}'
                    raise
                passed += 1
        finally:
            print(f'Native silent comparison planned={planned} attempted={attempted} passed={passed} '
                  f'failed={failed} not_run={planned-attempted} first_failure={first_failure}', flush=True)
        self.assertEqual((attempted, passed, failed), (planned, planned, 0))

    def scope_caller_text(self, marked=True):
        source = (ROOT / 'install/install.ps1').read_text(encoding='utf-8')
        # Exercise native preference scopes in the actual body. Only path/terminal
        # exit adapters change so a function can inspect caller state after return.
        body = source.replace('$PSScriptRoot', '$FixtureRoot').replace('exit $result', 'return $result').replace('exit 2', 'return 2')
        phase = lambda value: f'Write-ScopePhase {value}' if marked else ''
        definition = ('function Write-ScopePhase($Phase) { [Console]::Error.WriteLine("scope-fixture " + [DateTime]::UtcNow.ToString("o") + " " + $Phase) }\n'
                      + phase('"script-start"') + '\n') if marked else ''
        return ('param($FixtureRoot)\n' + definition + 'function Invoke-ActualLauncher {\n' + body
                + '\n}\nforeach($choice in @($true,$false)) {\n'
                + phase('("before-choice="+$choice);')
                + '$PSNativeCommandUseErrorActionPreference=$choice;$LASTEXITCODE=91;'
                + 'Invoke-ActualLauncher;'
                + phase('("after-choice="+$choice);')
                + 'if($PSNativeCommandUseErrorActionPreference -ne $choice -or $LASTEXITCODE -ne 91){throw "Caller state changed"}\n'
                + '}\n' + (phase('"script-end"') + '\n' if marked else ''))

    def write_scope_fixture(self):
        fixture = self.root / 'scope fixture'
        fixture.mkdir()
        (fixture / 'install.py').write_text('import sys;sys.exit(7)', encoding='utf-8')
        caller = self.root / 'scope.ps1'
        caller.write_text(self.scope_caller_text(), encoding='utf-8')
        return caller, fixture

    def run_scope_fixture(self, shell, caller, fixture, label, env, observation):
        name = Path(shell).name
        began = time.monotonic()
        logs = self.root / 'logs'
        prior_logs = set(logs.glob('*.log'))
        stdout, stderr, state = b'', b'', 'incomplete'
        try:
            with patch.dict(os.environ, {'REVAYAT_LOG_LEVEL': 'DEBUG'}), operational_log('scope-native-owner'):
                result = run_child([shell, '-NoProfile', '-NonInteractive', '-File', str(caller), str(fixture)],
                                   env=None if env is None else {**os.environ, **env}, timeout=20)
            stdout, stderr, state = result.stdout, result.stderr, 'complete'
            self.assertEqual(result.returncode, 0, result.stderr.decode('utf-8'))
        except subprocess.TimeoutExpired as error:
            stdout, stderr, state = error.output or b'', error.stderr or b'', 'timeout'
            raise
        finally:
            elapsed = time.monotonic() - began
            text = (f'Authored scope fixture shell={name} sample={label} state={state} elapsed_seconds={elapsed:.3f} wall=20 idle=10\n'
                    + 'stdout=' + stdout[:65536].decode('utf-8', errors='replace') + '\n'
                    + 'stderr=' + stderr[:65536].decode('utf-8', errors='replace'))
            diagnostics = logs / ('scope-capture-' + name + '-' + label + '.log')
            try:
                diagnostics.parent.mkdir(exist_ok=True)
                diagnostics.write_text(text, encoding='utf-8')
            except OSError:
                print('Native scope capture file unavailable; original outcome preserved', flush=True)
            print(f'Native scope shell={name} sample={label} state={state} elapsed_seconds={elapsed:.3f} wall=20 idle=10', flush=True)
            for line in stderr[:65536].decode('utf-8', errors='replace').splitlines():
                if line.startswith('scope-fixture ') and line.isascii():
                    observation['phase'] = line.split(' ', 2)[-1]
                    print(line, flush=True)
            if state == 'timeout':
                prefixes = ('Tool started ', 'Tool launch gate released ', 'Tool deadline expired ', 'Tool finished ',
                            'Supervisor launch gate validated', 'Supervisor target launch starting',
                            'Supervisor target started ', 'Supervisor target wait completed ', 'Supervised target exited ')
                candidates = sorted(path for path in set(logs.glob('*.log')) - prior_logs
                                    if path.name.startswith(('scope-native-owner_', 'process-supervisor_')))
                for path in candidates[:3]:
                    try:
                        for line in read_limited(path, 65536).decode('utf-8').splitlines():
                            message = line.split('] ', 3)[-1]
                            if message.startswith(prefixes) and message.isascii():
                                print('Native lifecycle ' + message, flush=True)
                    except (OSError, ValueError):
                        print('Native lifecycle capture unavailable; original timeout preserved', flush=True)
                bootstrap = sorted(path for path in set(logs.glob('*.log')) - prior_logs
                                   if path.name.startswith('install-bootstrap_'))
                for path in bootstrap[:2]:
                    try:
                        for line in read_limited(path, 65536).decode('utf-8').splitlines():
                            message = line.split('] ', 3)[-1]
                            if message in ('Checking Python prerequisite.', 'Installer completed exit=7.'):
                                print('Native bootstrap ' + message, flush=True)
                    except (OSError, ValueError):
                        print('Native bootstrap capture unavailable; original timeout preserved', flush=True)


if __name__ == "__main__":
    unittest.main()
