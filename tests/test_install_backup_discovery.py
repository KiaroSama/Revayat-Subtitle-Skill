"""New backups are retained outside skill discovery; custom roots are explicit."""
import unittest

from check import WorkspaceCase
from test_installation import installer


class InstallBackupDiscoveryTests(WorkspaceCase):
    def test_standard_root_has_one_active_skill_and_complete_old_backup(self):
        module = installer()
        target = self.root / ".agents/skills/revayat-subtitle"
        target.mkdir(parents=True)
        (target / "SKILL.md").write_text("---\nname: revayat-subtitle\n---\nold\n", encoding="utf-8")
        (target / "empty").mkdir()
        old = (target / "SKILL.md").read_bytes()
        backup = module.install(target, plugin=False, force=True)
        self.assertFalse(backup.is_relative_to(target.parent))
        self.assertEqual((backup / "SKILL.md").read_bytes(), old)
        self.assertTrue((backup / "empty").is_dir())
        self.assertEqual(list(target.parent.glob("**/SKILL.md")), [target / "SKILL.md"])

    def test_absent_standard_agent_root_is_created_with_owned_identity(self):
        module = installer()
        target = self.root / '.claude/skills/revayat-subtitle'
        self.assertFalse(target.parent.parent.exists())
        result = module.install(target, plugin=False, force=False)
        self.assertIsNone(result)
        self.assertEqual((target / 'SKILL.md').read_bytes(), (module.SKILL / 'SKILL.md').read_bytes())
        self.assertTrue((target.parent.parent / 'revayat-recovery').is_dir())

    def test_foreign_agent_ancestor_created_after_plan_is_not_adopted(self):
        module = installer()
        target = self.root / '.claude/skills/revayat-subtitle'
        plan = module.plan_install([target], plugin=False, force=False)
        target.parent.parent.mkdir()
        sentinel = target.parent.parent / 'foreign.txt'
        sentinel.write_bytes(b'keep')
        with self.assertRaisesRegex(ValueError, 'ancestor changed'):
            module.execute_plan(plan)
        self.assertEqual(sentinel.read_bytes(), b'keep')
        self.assertFalse(target.exists())

    def test_custom_root_requires_recovery_and_refuses_discovery_overlap(self):
        module = installer()
        target = self.root / "custom"
        with self.assertRaisesRegex(ValueError, "recovery-dir"):
            module.plan_install([target], plugin=False, force=False)
        with self.assertRaisesRegex(ValueError, "discovery"):
            module.plan_install([target], plugin=False, force=False, recovery_dir=self.root / "skills/recovery")
        self.assertFalse(target.exists())


if __name__ == "__main__":
    unittest.main()
