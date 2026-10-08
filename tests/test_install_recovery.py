"""Owned interrupted install recovery, exact old trees and stable OS contention."""
import os
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

from check import WorkspaceCase, ROOT
from process_helpers import run_child
from test_installation import installer


class InstallRecoveryTests(WorkspaceCase):
    def test_native_process_death_after_old_move_restores_exact_old(self):
        target = self.root / "target"
        target.mkdir()
        (target / "old.txt").write_bytes(b"original old bytes")
        recovery = self.root / "recovery"
        code = ("import importlib.util,os,sys;from pathlib import Path;"
                "spec=importlib.util.spec_from_file_location('owned_install',sys.argv[1]);"
                "m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);real=m.rename_noreplace\n"
                "def stop(source,destination):\n real(source,destination)\n"
                " if Path(destination).name.startswith('old-'): os._exit(73)\n"
                "m.rename_noreplace=stop\nm.install(Path(sys.argv[2]),plugin=False,force=True,recovery_dir=Path(sys.argv[3]))")
        result = run_child([sys.executable, "-B", "-c", code, str(ROOT / "install/install.py"), str(target), str(recovery)], timeout=15)
        self.assertEqual(result.returncode, 73, result.stderr.decode("utf-8"))
        journals = list(recovery.glob("transaction-*/journal.json"))
        self.assertEqual(len(journals), 1)
        self.assertFalse(target.exists())
        module = installer()
        with self.assertRaisesRegex(ValueError, "explicit --recover"):
            module.install(target, plugin=False, force=True, recovery_dir=self.root / "other recovery")
        module.recover(journals[0], recovery, module.rename_noreplace)
        self.assertEqual((target / "old.txt").read_bytes(), b"original old bytes")
        self.assertFalse((target / "SKILL.md").exists())
        module.recover(journals[0], recovery, module.rename_noreplace)
        self.assertEqual((target / "old.txt").read_bytes(), b"original old bytes")

    def test_stable_lock_contention_is_nonblocking_and_not_unlinked(self):
        module = installer()
        with module.state.ownership() as root:
            lock = root / "owner.lock"
            before = module.state.identity(lock)
            code = "import sys;sys.path[:0]=[sys.argv[1],sys.argv[2]];import install_state\nwith install_state.ownership(): pass"
            result = run_child([sys.executable, "-B", "-c", code, str(ROOT / "install"), str(ROOT / "skills/revayat-subtitle/scripts")], timeout=8)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b"OS lock", result.stderr)
            self.assertEqual(module.state.identity(lock), before)
        self.assertTrue(lock.is_file())

    def test_after_effect_old_move_exception_restores_and_preserves_exception(self):
        module = installer()
        target = self.root / "target"
        target.mkdir()
        (target / "old.txt").write_bytes(b"original")
        real = module.rename_noreplace
        error = OSError("after-effect")
        fired = False
        def fail(source, destination):
            nonlocal fired
            real(source, destination)
            if destination.name.startswith("old-") and not fired:
                fired = True
                raise error
        with patch.object(module, "rename_noreplace", fail), self.assertRaises(OSError) as caught:
            module.install(target, plugin=False, force=True, recovery_dir=self.root / "recovery")
        self.assertIs(caught.exception, error)
        self.assertEqual((target / "old.txt").read_bytes(), b"original")

    def test_committed_recovery_refuses_uninstall_and_preserves_backup(self):
        module = installer()
        target = self.root / "target"
        target.mkdir()
        (target / "old.txt").write_bytes(b"original")
        backup = module.install(target, plugin=False, force=True, recovery_dir=self.root / "recovery")
        with self.assertRaisesRegex(ValueError, "Committed"):
            module.recover(backup.parent / "journal.json", self.root / "recovery", module.rename_noreplace)
        self.assertEqual((backup / "old.txt").read_bytes(), b"original")
        self.assertTrue((target / "SKILL.md").is_file())


    def interrupted_publication(self, *, old=True, point='new'):
        target = self.root / ('target-' + point)
        recovery = self.root / ('recovery-' + point)
        if old:
            target.mkdir()
            (target / 'old.txt').write_bytes(b'original')
        code = ("import importlib.util,os,sys;from pathlib import Path;"
                "spec=importlib.util.spec_from_file_location('owned_install',sys.argv[1]);"
                "m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);real=m.rename_noreplace\n"
                "def stop(source,destination):\n real(source,destination)\n"
                " if Path(destination)==Path(sys.argv[2]): os._exit(74)\n"
                "m.rename_noreplace=stop\n"
                "m.install(Path(sys.argv[2]),plugin=False,force=True,recovery_dir=Path(sys.argv[3]))")
        result = run_child([sys.executable, '-B', '-c', code, str(ROOT / 'install/install.py'), str(target), str(recovery)], timeout=15)
        self.assertEqual(result.returncode, 74, result.stderr.decode('utf-8'))
        return target, recovery, next(recovery.glob('transaction-*/journal.json'))

    def test_rewritten_valid_journal_cannot_authorize_foreign_tree_deletion(self):
        from runtime import read_json, write_json
        target, recovery, journal = self.interrupted_publication(old=False)
        module = installer()
        foreign = self.root / 'foreign'
        foreign.mkdir()
        (foreign / 'keep.txt').write_bytes(b'untouched')
        data = read_json(journal)
        data['records'][0].update(target=str(foreign), stage_id=module.state.identity(foreign), new=module.state.tree_manifest(foreign))
        write_json(journal, data)
        with self.assertRaisesRegex(ValueError, 'instructions'):
            module.recover(journal, recovery, module.rename_noreplace)
        self.assertEqual((foreign / 'keep.txt').read_bytes(), b'untouched')
        self.assertTrue((target / 'SKILL.md').is_file())

    def test_recovery_death_during_quarantine_cleanup_is_restartable(self):
        target, recovery, journal = self.interrupted_publication()
        code = ("import importlib.util,os,sys;from pathlib import Path;"
                "spec=importlib.util.spec_from_file_location('owned_install',sys.argv[1]);"
                "m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);import install_recovery as r;real=r.shutil.rmtree\n"
                "def stop(path,*args,**kwargs):\n"
                " if Path(path).name.startswith('dispose-'):\n"
                "  first=next(Path(path).rglob('*.py'));first.unlink();os._exit(75)\n"
                " return real(path,*args,**kwargs)\n"
                "r.shutil.rmtree=stop\nm.recover(Path(sys.argv[2]),Path(sys.argv[3]),m.rename_noreplace)")
        result = run_child([sys.executable, '-B', '-c', code, str(ROOT / 'install/install.py'), str(journal), str(recovery)], timeout=15)
        self.assertEqual(result.returncode, 75, result.stderr.decode('utf-8'))
        self.assertEqual((target / 'old.txt').read_bytes(), b'original')
        module = installer()
        module.recover(journal, recovery, module.rename_noreplace)
        self.assertEqual((target / 'old.txt').read_bytes(), b'original')

    def test_all_record_preflight_refuses_changed_backup_before_any_removal(self):
        module = installer()
        targets = [self.root / 'first', self.root / 'second']
        for target in targets:
            target.mkdir()
            (target / 'old.txt').write_bytes(b'old')
        recovery = self.root / 'multiple recovery'
        code = ("import importlib.util,os,sys;from pathlib import Path;"
                "s=importlib.util.spec_from_file_location('i',sys.argv[1]);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);real=m.rename_noreplace\n"
                "def stop(a,b):\n real(a,b)\n if Path(b)==Path(sys.argv[3]): os._exit(76)\n"
                "m.rename_noreplace=stop\nm.execute_plan(m.plan_install([Path(sys.argv[2]),Path(sys.argv[3])],plugin=False,force=True,recovery_dir=Path(sys.argv[4])))")
        result = run_child([sys.executable, '-B', '-c', code, str(ROOT / 'install/install.py'),
                            str(targets[0]), str(targets[1]), str(recovery)], timeout=15)
        self.assertEqual(result.returncode, 76, result.stderr.decode('utf-8'))
        journal = next(recovery.glob('transaction-*/journal.json'))
        from runtime import read_json
        data = read_json(journal)
        backup = Path(data['records'][1]['backup'])
        (backup / 'old.txt').write_bytes(b'changed')
        with self.assertRaisesRegex(ValueError, 'backup changed'):
            module.recover(journal, recovery, module.rename_noreplace)
        self.assertTrue(all((target / 'SKILL.md').is_file() for target in targets))
        self.assertEqual((backup / 'old.txt').read_bytes(), b'changed')

    def test_changed_stage_and_ancestor_refuse_without_old_tree_mutation(self):
        from runtime import read_json
        module = installer()
        target, recovery, journal = self.interrupted_publication()
        data = read_json(journal)
        backup = Path(data['records'][0]['backup'])
        original = (backup / 'old.txt').read_bytes()
        stage = Path(data['records'][0]['stage'])
        stage.mkdir()
        (stage / 'foreign').write_bytes(b'keep')
        with self.assertRaisesRegex(ValueError, 'staging identity'):
            module.recover(journal, recovery, module.rename_noreplace)
        self.assertEqual((stage / 'foreign').read_bytes(), b'keep')
        self.assertEqual((backup / 'old.txt').read_bytes(), original)
        stage.rename(self.root / 'preserved foreign stage')
        parent = target.parent
        # Replace only an isolated target ancestor in a separate controlled transaction.
        second_parent = self.root / 'changed parent'
        second_parent.mkdir()
        second_target = second_parent / 'target'
        second_target.mkdir()
        (second_target / 'old.txt').write_bytes(b'old')
        code = ("import importlib.util,os,sys;from pathlib import Path;s=importlib.util.spec_from_file_location('i',sys.argv[1]);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);real=m.rename_noreplace\n"
                "def stop(a,b):\n real(a,b)\n if Path(b).name.startswith('old-'): os._exit(78)\n"
                "m.rename_noreplace=stop\nm.install(Path(sys.argv[2]),plugin=False,force=True,recovery_dir=Path(sys.argv[3]))")
        second_recovery = self.root / 'ancestor recovery'
        result = run_child([sys.executable, '-B', '-c', code, str(ROOT / 'install/install.py'), str(second_target), str(second_recovery)], timeout=15)
        self.assertEqual(result.returncode, 78)
        second_parent.rename(self.root / 'original changed parent')
        second_parent.mkdir()
        with self.assertRaisesRegex(ValueError, 'ancestor changed'):
            module.recover(next(second_recovery.glob('transaction-*/journal.json')), second_recovery, module.rename_noreplace)
        self.assertEqual(list(second_parent.iterdir()), [])

    def test_unknown_stage_identity_window_refuses_and_preserves_old_bytes(self):
        target = self.root / 'unknown stage target'
        target.mkdir()
        (target / 'old.txt').write_bytes(b'old')
        recovery = self.root / 'unknown stage recovery'
        code = ("import importlib.util,os,sys;from pathlib import Path;s=importlib.util.spec_from_file_location('i',sys.argv[1]);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);real=m.state.write_json\n"
                "def stop(path,value):\n real(path,value)\n"
                " if Path(path).name=='pending.json' and any(len(v['digests'])==2 for v in value): os._exit(80)\n"
                "m.state.write_json=stop\nm.install(Path(sys.argv[2]),plugin=False,force=True,recovery_dir=Path(sys.argv[3]))")
        result = run_child([sys.executable, '-B', '-c', code, str(ROOT / 'install/install.py'), str(target), str(recovery)], timeout=15)
        self.assertEqual(result.returncode, 80, result.stderr.decode('utf-8'))
        journal = next(recovery.glob('transaction-*/journal.json'))
        module = installer()
        with self.assertRaisesRegex(ValueError, 'staging identity'):
            module.recover(journal, recovery, module.rename_noreplace)
        self.assertEqual((target / 'old.txt').read_bytes(), b'old')
        self.assertTrue((journal.parent / 'new-0000').is_dir())

    def test_registry_before_journal_update_death_accepts_only_prior_intended_revision(self):
        target = self.root / 'binding target'
        target.mkdir()
        (target / 'old.txt').write_bytes(b'old')
        recovery = self.root / 'binding recovery'
        code = ("import importlib.util,os,sys;from pathlib import Path;s=importlib.util.spec_from_file_location('i',sys.argv[1]);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);real=m.state.write_json\n"
                "def stop(path,value):\n real(path,value)\n"
                " if Path(path).name=='pending.json' and any(len(v['digests'])==2 for v in value) and any(Path(sys.argv[3]).glob('transaction-*/new-0000/SKILL.md')): os._exit(79)\n"
                "m.state.write_json=stop\nm.install(Path(sys.argv[2]),plugin=False,force=True,recovery_dir=Path(sys.argv[3]))")
        result = run_child([sys.executable, '-B', '-c', code, str(ROOT / 'install/install.py'), str(target), str(recovery)], timeout=15)
        self.assertEqual(result.returncode, 79, result.stderr.decode('utf-8'))
        module = installer()
        module.recover(next(recovery.glob('transaction-*/journal.json')), recovery, module.rename_noreplace)
        self.assertEqual((target / 'old.txt').read_bytes(), b'old')

    def test_death_after_bound_journal_persistence_has_exact_restart_outcome(self):
        target = self.root / 'persist target'
        target.mkdir()
        (target / 'old.txt').write_bytes(b'old')
        recovery = self.root / 'persist recovery'
        code = ("import importlib.util,os,sys;from pathlib import Path;"
                "s=importlib.util.spec_from_file_location('i',sys.argv[1]);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);real=m.state.save_journal\n"
                "def stop(root,path,data):\n real(root,path,data)\n if data['records'][0]['phase']=='new-intent': os._exit(77)\n"
                "m.state.save_journal=stop\nm.install(Path(sys.argv[2]),plugin=False,force=True,recovery_dir=Path(sys.argv[3]))")
        result = run_child([sys.executable, '-B', '-c', code, str(ROOT / 'install/install.py'), str(target), str(recovery)], timeout=15)
        self.assertEqual(result.returncode, 77, result.stderr.decode('utf-8'))
        module = installer()
        module.recover(next(recovery.glob('transaction-*/journal.json')), recovery, module.rename_noreplace)
        self.assertEqual((target / 'old.txt').read_bytes(), b'old')

    def test_corrupt_unknown_typed_journal_and_hex_identity_refuse(self):
        from runtime import read_json, write_json
        target, recovery, journal = self.interrupted_publication()
        module = installer()
        data = read_json(journal)
        for field, value in [('version', 99), ('state', []), ('recovery', {} )]:
            changed = dict(data)
            changed[field] = value
            write_json(journal, changed)
            with self.subTest(field=field), self.assertRaises(ValueError):
                module.state.load_journal(journal)
        journal.write_bytes(b'{')
        with self.assertRaises(ValueError):
            module.state.load_journal(journal)
        self.assertTrue(module.state.valid_identity(['1', 'f' * 32]))
        self.assertFalse(module.state.valid_identity(['1', 'f' * 33]))
        self.assertTrue((target / 'SKILL.md').exists())


if __name__ == "__main__":
    unittest.main()
