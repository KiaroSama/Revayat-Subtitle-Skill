"""Four-cell legacy-shell diagnostic for inherited cross-edition module paths."""
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
import unittest
from unittest.mock import patch

from check import WorkspaceCase
from process_helpers import run_child
from runtime import read_limited
import test_bootstrap_preferences as bootstrap


PHASE_WRITER = '''
function Write-ModulePhase([int]$Stage, [string]$Detail = "") {
    [IO.File]::AppendAllText($EvidenceFile,
        [DateTime]::UtcNow.ToString("o") + " " + $Stage + " " + $Detail + "`n",
        [Text.UTF8Encoding]::new($false))
}
Write-ModulePhase 0
'''


def diagnostic_caller(case, scope):
    if scope:
        caller = bootstrap.BootstrapPreferenceTests.scope_caller_text(case, marked=False)
        boundaries = (
            ("$installer = Join-Path $FixtureRoot 'install.py'", 10, 11),
            ("$script:writer = New-Object IO.StreamWriter($stream, (New-Object Text.UTF8Encoding($false)))", 20, 21),
            ("$command = Get-Command $candidate -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1", 30, 31),
            ("$probe = Invoke-NativeStatus $command.Source (@($prefix) + @('-B', '-c', 'import sys; sys.exit(0 if sys.version_info >= (3, 11) else 1)')) $true", 40, 41),
            ("$result = Invoke-NativeStatus $command.Source (@($prefix) + @('-B', '-X', 'utf8', $installer) + @($forwarded)) $false", 50, 51),
        )
        for statement, before, after in boundaries:
            if caller.count(statement) != 1:
                raise AssertionError('Native module diagnostic boundary changed')
            caller = caller.replace(statement, f'Write-ModulePhase {before};' + statement
                                    + f';Write-ModulePhase {after}')
        recovered = re.sub(r'Write-ModulePhase (?:10|20|30|40|50);|;Write-ModulePhase (?:11|21|31|41|51)', '', caller)
        if recovered != bootstrap.BootstrapPreferenceTests.scope_caller_text(case, marked=False):
            raise AssertionError('Native module diagnostic changed the silent scope contract')
        body = caller.split('\n', 1)[1]
        # This query is after the real scope assertions, not a startup prerequisite.
        body += '''
Write-ModulePhase 90
$resolved = Get-Command Join-Path -CommandType Cmdlet -ErrorAction Stop
$majorAtMostThree = 2
if ($null -ne $resolved.Module) {
    $majorAtMostThree = [int]($resolved.Module.Version.Major -le 3)
}
Write-ModulePhase 95 ([string]$majorAtMostThree)
'''
    else:
        body = 'Write-ModulePhase 90\n'
    return ('param($FixtureRoot,$EvidenceFile)\n' + PHASE_WRITER + 'try {\n' + body
            + '} catch {\n$originalError = $_\ntry { Write-ModulePhase 99 ($originalError.Exception.GetType().FullName) } catch {}\nthrow $originalError\n}\n')


@unittest.skipUnless(os.name == 'nt', 'Windows legacy module environment diagnostic')
class NativeModuleEnvironmentTests(WorkspaceCase):
    def test_fixed_module_environment_cells(self):
        # Order is fixed, not repeated until success. First use is home-state only.
        cells = (('dotnet-inherited', False, False), ('dotnet-omitted', False, True),
                 ('scope-omitted', True, True), ('scope-inherited', True, False))
        planned, attempted, passed, failed = len(cells), 0, 0, 0
        first_failure = 'none'
        last_phase = 'not-observed'
        try:
            shell = shutil.which('powershell.exe')
            if shell is None:
                first_failure = 'prerequisite:powershell.exe'
                self.fail('Native module diagnostic requires powershell.exe')
            for label, scope, omit in cells:
                cell = self.root / label
                cell.mkdir()
                home = cell / 'home فارسی'
                home.mkdir()
                fixture = cell / 'scope fixture'
                fixture.mkdir()
                (fixture / 'install.py').write_text('import sys;sys.exit(7)', encoding='utf-8')
                caller = cell / 'scope.ps1'
                text = diagnostic_caller(self, scope)
                self.assertNotIn('Write-ScopePhase', text)
                caller.write_text(text, encoding='utf-8')
                evidence = cell / 'phases.log'
                last_phase = 'not-observed'
                attempted += 1
                began = time.monotonic()
                logs = self.root / 'logs'
                prior_logs = set(logs.glob('*.log'))
                timed_out = False
                try:
                    with patch.dict(os.environ, {'HOME': str(home), 'USERPROFILE': str(home)}), \
                         patch.object(Path, 'home', return_value=home):
                        env = dict(os.environ)
                        if omit:
                            env = {key: value for key, value in env.items() if key.upper() != 'PSMODULEPATH'}
                        result = run_child([shell, '-NoProfile', '-NonInteractive', '-File', str(caller),
                                            str(fixture), str(evidence)], env=env, timeout=20)
                    self.assertEqual(result.returncode, 0, 'Native module diagnostic failed; inspect numeric phases')
                    observed = read_limited(evidence, 16384).decode('utf-8').splitlines()
                    rows = [line.split(' ', 2) for line in observed]
                    self.assertTrue(all(len(row) == 3 for row in rows), 'Invalid authored module phase record')
                    expected = ([0] + [10, 11, 20, 21, 30, 31, 40, 41, 50, 51] * 2 + [90, 95]
                                if scope else [0, 90])
                    self.assertEqual([int(row[1]) for row in rows], expected)
                    if scope:
                        # This is only the observed major-version predicate, not compatibility proof.
                        self.assertIn(rows[-1][2], ('0', '1', '2'))
                    passed += 1
                except BaseException as error:
                    failed += 1
                    first_failure = f'{label}:{type(error).__name__}'
                    timed_out = isinstance(error, subprocess.TimeoutExpired)
                    raise
                finally:
                    print(f'Native module cell={label} elapsed_seconds={time.monotonic()-began:.3f} '
                          'wall=20 idle=10', flush=True)
                    try:
                        for line in read_limited(evidence, 16384).decode('utf-8').splitlines():
                            if re.fullmatch(r'[0-9T:.+Z-]+ (?:0|10|11|20|21|30|31|40|41|50|51|90|95|99) [A-Za-z0-9_.]*', line):
                                last_phase = line.split(' ')[1]
                                print(f'Native module cell={label} phase={line}', flush=True)
                    except (OSError, ValueError):
                        print(f'Native module cell={label} phase=unavailable', flush=True)
                    if timed_out:
                        print('Native module deadline reason=unavailable unless existing log reports it', flush=True)
                        prefixes = ('Tool launch gate released ', 'Tool deadline expired ',
                                    'Supervisor launch gate validated', 'Supervisor target launch starting',
                                    'Supervisor target started ', 'Supervisor target wait completed ')
                        try:
                            current = sorted(path for path in set(logs.glob('*.log')) - prior_logs
                                             if path.name.startswith('process-supervisor_'))
                            for path in current[:1]:
                                for line in read_limited(path, 65536).decode('utf-8').splitlines():
                                    message = line.split('] ', 3)[-1]
                                    if message.startswith(prefixes) and message.isascii():
                                        print('Native module lifecycle ' + message, flush=True)
                        except (OSError, ValueError):
                            print('Native module lifecycle unavailable; original timeout preserved', flush=True)
        finally:
            print(f'Native module cells planned={planned} attempted={attempted} passed={passed} '
                  f'failed={failed} not_run={planned-attempted} first_failure={first_failure} '
                  f'last_phase={last_phase}', flush=True)
        self.assertEqual((attempted, passed, failed), (planned, planned, 0))


if __name__ == '__main__':
    unittest.main()
