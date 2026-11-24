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


    def interrupted_publication(self, *, old=True, point='new', parent=None):
        parent = self.root if parent is None else parent
        parent.mkdir(exist_ok=True)
        target = parent / ('target-' + point)
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
        self.registry_pressure(module, journal)
        module.install(self.root / 'after-cleanup-crash', plugin=False, force=False,
                       recovery_dir=recovery)
        with module.state.ownership() as owner:
            self.assertTrue(module.state.registered(owner, journal)['active'])
        module.recover(journal, recovery, module.rename_noreplace)
        self.assertEqual((target / 'old.txt').read_bytes(), b'original')

    def registry_pressure(self, module, journal, *, inactive=False, count=1000):
        from runtime import write_json
        with module.state.ownership() as owner:
            binding = module.state.registered(owner, journal)
            if inactive:
                binding['active'] = False
            history = [{'path': str(self.root / f'past-{index}' / 'journal.json'),
                        'transaction_id': ['1', f'{index + 1:x}'],
                        'digests': ['0' * 64], 'active': False}
                       for index in range(count - 1)]
            write_json(owner / 'pending.json', [binding, *history])
        return owner

    def test_death_after_restored_journal_keeps_cleanup_authorization_under_pressure(self):
        target, recovery, journal = self.interrupted_publication(point='restored-crash')
        code = ("import importlib.util,os,sys;from pathlib import Path;"
                "spec=importlib.util.spec_from_file_location('owned_install',sys.argv[1]);"
                "m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);real=m.state.save_journal\n"
                "def stop(owner,journal,data):\n real(owner,journal,data)\n"
                " if data['state']=='restored': os._exit(77)\n"
                "m.state.save_journal=stop\nm.recover(Path(sys.argv[2]),Path(sys.argv[3]),m.rename_noreplace)")
        result = run_child([sys.executable, '-B', '-c', code, str(ROOT / 'install/install.py'),
                            str(journal), str(recovery)], timeout=15)
        self.assertEqual(result.returncode, 77, result.stderr.decode('utf-8'))
        module = installer()
        owner = self.registry_pressure(module, journal)
        module.install(self.root / 'next-install', plugin=False, force=False, recovery_dir=recovery)
        self.assertTrue(module.state.registered(owner, journal)['active'])
        self.assertEqual((target / 'old.txt').read_bytes(), b'original')
        module.recover(journal, recovery, module.rename_noreplace)
        self.assertFalse(module.state.registered(owner, journal)['active'])
        self.assertFalse((journal.parent / 'dispose-0000').exists())

    def pending_disposal(self, *, old=True, point='pending', parent=None):
        from runtime import read_json
        target, recovery, journal = self.interrupted_publication(old=old, point=point, parent=parent)
        module = installer()
        recovery_module = sys.modules['install_recovery']
        real = recovery_module.shutil.rmtree

        def fail(path, *args, **kwargs):
            if Path(path).name.startswith('dispose-'):
                raise PermissionError('Injected disposal cleanup denial')
            return real(path, *args, **kwargs)

        with patch.object(recovery_module.shutil, 'rmtree', fail):
            module.recover(journal, recovery, module.rename_noreplace)
        data = read_json(journal)
        self.assertEqual(data['state'], 'restored')
        disposal = Path(data['records'][0]['disposal'])
        self.assertTrue(disposal.is_dir())
        return module, target, recovery, journal, disposal

    def test_restored_disposal_survives_capacity_and_a_later_target_install(self):
        for old in (True, False):
            for legacy_inactive in (True, False):
                with self.subTest(old=old, legacy_inactive=legacy_inactive):
                    module, target, recovery, journal, disposal = self.pending_disposal(
                        old=old, point=f'pressure-{old}-{legacy_inactive}')
                    owner = self.registry_pressure(module, journal, inactive=legacy_inactive)
                    if old:
                        self.assertEqual((target / 'old.txt').read_bytes(), b'original')
                    else:
                        self.assertFalse(target.exists())
                    module.install(target, plugin=False, force=True, recovery_dir=recovery)
                    self.assertTrue(module.state.registered(owner, journal))
                    self.assertTrue(disposal.is_dir())
                    current_id = module.state.identity(target)
                    current_tree = module.state.tree_manifest(target)
                    module.recover(journal, recovery, module.rename_noreplace)
                    module.recover(journal, recovery, module.rename_noreplace)
                    self.assertFalse(os.path.lexists(disposal))
                    self.assertFalse(module.state.registered(owner, journal)['active'])
                    self.assertEqual(module.state.identity(target), current_id)
                    self.assertEqual(module.state.tree_manifest(target), current_tree)
                    # Once cleanup succeeds, a later capacity cycle may retire it.
                    module.install(self.root / f'next-{old}-{legacy_inactive}', plugin=False,
                                   force=False, recovery_dir=recovery)
                    with self.assertRaisesRegex(ValueError, 'instructions'):
                        module.state.registered(owner, journal)

    def test_legacy_overflow_keeps_pending_cleanup_until_owned_retry_finishes(self):
        module, target, recovery, journal, disposal = self.pending_disposal()
        owner = self.registry_pressure(module, journal, inactive=True, count=1001)
        recovery_module = sys.modules['install_recovery']
        with patch.object(recovery_module.shutil, 'rmtree', side_effect=PermissionError('cleanup denied')):
            module.recover(journal, recovery, module.rename_noreplace)
        self.assertTrue(module.state.registered(owner, journal)['active'])
        self.assertEqual(len(module.state.registry(owner)), 1000)
        self.assertTrue(disposal.is_dir())
        self.assertEqual((target / 'old.txt').read_bytes(), b'original')
        module.recover(journal, recovery, module.rename_noreplace)
        self.assertFalse(disposal.exists())

    def test_replaced_disposal_is_preserved_and_its_binding_cannot_be_retired(self):
        module, target, recovery, journal, disposal = self.pending_disposal()
        original = disposal.with_name('kept-owned-disposal')
        disposal.rename(original)
        disposal.mkdir()
        (disposal / 'foreign.txt').write_bytes(b'foreign bytes')
        owner = self.registry_pressure(module, journal, inactive=True)
        module.install(self.root / 'next-install', plugin=False, force=False, recovery_dir=recovery)
        module.recover(journal, recovery, module.rename_noreplace)
        self.assertTrue(module.state.registered(owner, journal)['active'])
        self.assertEqual((disposal / 'foreign.txt').read_bytes(), b'foreign bytes')
        self.assertEqual((target / 'old.txt').read_bytes(), b'original')
        self.assertTrue(original.is_dir())

    def test_broken_disposal_symlink_is_preserved_with_its_legacy_binding(self):
        module, target, recovery, journal, disposal = self.pending_disposal()
        disposal.rename(disposal.with_name('kept-owned-disposal'))
        try:
            disposal.symlink_to(self.root / 'absent-foreign-directory', target_is_directory=True)
        except (OSError, NotImplementedError):
            self.skipTest('Host cannot create directory symlinks')
        owner = self.registry_pressure(module, journal, inactive=True)
        module.install(self.root / 'next-install', plugin=False, force=False, recovery_dir=recovery)
        module.recover(journal, recovery, module.rename_noreplace)
        self.assertTrue(module.state.registered(owner, journal)['active'])
        self.assertTrue(disposal.is_symlink())
        self.assertEqual((target / 'old.txt').read_bytes(), b'original')

    def test_unreadable_disposal_metadata_keeps_legacy_cleanup_authorization(self):
        module, target, recovery, journal, disposal = self.pending_disposal()
        owner = self.registry_pressure(module, journal, inactive=True)
        real_lstat = Path.lstat

        def denied(path, *args, **kwargs):
            if path == disposal:
                raise PermissionError('Injected metadata denial')
            return real_lstat(path, *args, **kwargs)

        with patch.object(Path, 'lstat', denied):
            module.install(self.root / 'next-install', plugin=False, force=False, recovery_dir=recovery)
            module.recover(journal, recovery, module.rename_noreplace)
            self.assertTrue(module.state.registered(owner, journal)['active'])
        self.assertEqual((target / 'old.txt').read_bytes(), b'original')
        module.recover(journal, recovery, module.rename_noreplace)
        self.assertFalse(disposal.exists())

    def test_changed_inactive_journal_is_not_retired_as_completed(self):
        module, target, recovery, journal, disposal = self.pending_disposal()
        owner = self.registry_pressure(module, journal, inactive=True)
        journal.write_bytes(journal.read_bytes() + b'\n')
        module.install(self.root / 'next-install', plugin=False, force=False, recovery_dir=recovery)
        self.assertIn(str(journal), [entry['path'] for entry in module.state.registry(owner)])
        with self.assertRaisesRegex(ValueError, 'instructions'):
            module.recover(journal, recovery, module.rename_noreplace)
        self.assertTrue(disposal.is_dir())
        self.assertEqual((target / 'old.txt').read_bytes(), b'original')

    def test_completed_history_can_retire_after_its_target_parent_is_gone(self):
        module = installer()
        target, recovery = self.root / 'obsolete-parent' / 'target', self.root / 'recovery'
        module.install(target, plugin=False, force=False, recovery_dir=recovery)
        journal = next(recovery.glob('transaction-*/journal.json'))
        owner = self.registry_pressure(module, journal)
        module.shutil.rmtree(target.parent)
        module.install(self.root / 'next-install', plugin=False, force=False, recovery_dir=recovery)
        self.assertNotIn(str(journal), [entry['path'] for entry in module.state.registry(owner)])

    def test_restored_cleanup_ignores_a_relocated_live_target_parent(self):
        parent = self.root / 'obsolete-live-parent'
        module, target, recovery, journal, disposal = self.pending_disposal(parent=parent)
        moved = self.root / 'preserved-live-parent'
        parent.rename(moved)
        old_bytes = (moved / target.name / 'old.txt').read_bytes()
        module.install(self.root / 'unrelated-install', plugin=False, force=False,
                       recovery_dir=self.root / 'unrelated-recovery')
        module.recover(journal, recovery, module.rename_noreplace)
        module.recover(journal, recovery, module.rename_noreplace)
        self.assertFalse(disposal.exists())
        self.assertFalse(parent.exists())
        self.assertEqual((moved / target.name / 'old.txt').read_bytes(), old_bytes)

    def test_restored_cleanup_never_follows_a_replaced_live_parent_symlink(self):
        parent = self.root / 'obsolete-live-parent'
        module, target, recovery, journal, disposal = self.pending_disposal(parent=parent)
        parent.rename(self.root / 'preserved-live-parent')
        foreign = self.root / 'foreign-parent'
        foreign.mkdir()
        (foreign / 'keep.txt').write_bytes(b'foreign bytes')
        try:
            parent.symlink_to(foreign, target_is_directory=True)
        except (OSError, NotImplementedError):
            self.skipTest('Host cannot create directory symlinks')
        module.install(self.root / 'unrelated-install', plugin=False, force=False,
                       recovery_dir=self.root / 'unrelated-recovery')
        module.recover(journal, recovery, module.rename_noreplace)
        self.assertFalse(disposal.exists())
        self.assertTrue(parent.is_symlink())
        self.assertEqual(module.state.tree_manifest(foreign)['files'].keys(), {'keep.txt'})
        self.assertEqual((foreign / 'keep.txt').read_bytes(), b'foreign bytes')

    def test_committed_reconciliation_ignores_an_obsolete_live_target_parent(self):
        module = installer()
        parent, recovery = self.root / 'obsolete-live-parent', self.root / 'recovery'
        target = parent / 'target'
        target.mkdir(parents=True)
        (target / 'old.txt').write_bytes(b'original')
        with patch.object(module.state, 'unregister', side_effect=OSError('commit unregister interrupted')):
            with self.assertRaisesRegex(OSError, 'commit unregister interrupted'):
                module.install(target, plugin=False, force=True, recovery_dir=recovery)
        journal = next(recovery.glob('transaction-*/journal.json'))
        moved = self.root / 'preserved-live-parent'
        parent.rename(moved)
        expected = module.state.tree_manifest(moved / 'target')
        module.install(self.root / 'unrelated-install', plugin=False, force=False,
                       recovery_dir=self.root / 'unrelated-recovery')
        with module.state.ownership() as owner:
            self.assertFalse(module.state.registered(owner, journal)['active'])
        with self.assertRaisesRegex(ValueError, 'Committed'):
            module.recover(journal, recovery, module.rename_noreplace)
        self.assertEqual(module.state.tree_manifest(moved / 'target'), expected)
        self.assertEqual((journal.parent / 'old-0000' / 'old.txt').read_bytes(), b'original')

    def test_terminal_recovery_ancestor_replacement_still_refuses(self):
        from runtime import write_json
        for terminal in ('committed', 'restored'):
            with self.subTest(terminal=terminal):
                module = installer()
                with module.state.ownership() as owner:
                    write_json(owner / 'pending.json', [])
                if terminal == 'restored':
                    module, target, recovery, journal, disposal = self.pending_disposal(point=terminal)
                else:
                    module = installer()
                    target, recovery = self.root / 'target-committed', self.root / 'recovery-committed'
                    module.install(target, plugin=False, force=False, recovery_dir=recovery)
                    journal = next(recovery.glob('transaction-*/journal.json'))
                with module.state.ownership() as owner:
                    binding = module.state.registered(owner, journal)
                    binding['active'] = True
                    write_json(owner / 'pending.json', [binding])
                before = module.state.tree_manifest(target)
                held = self.root / f'held-recovery-{terminal}'
                recovery.rename(held)
                recovery.mkdir()
                (held / journal.parent.name).rename(journal.parent)
                self.assertTrue(module.state.registered(owner, journal))
                with self.assertRaisesRegex(ValueError, 'ancestor changed'):
                    module.recover(journal, recovery, module.rename_noreplace)
                unrelated = self.root / f'unrelated-{terminal}'
                with self.assertRaisesRegex(ValueError, 'ancestor changed'):
                    module.install(unrelated, plugin=False, force=False,
                                   recovery_dir=self.root / f'unrelated-recovery-{terminal}')
                self.assertFalse(unrelated.exists())
                self.assertEqual(module.state.tree_manifest(target), before)
                if terminal == 'restored':
                    self.assertTrue(disposal.is_dir())

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
