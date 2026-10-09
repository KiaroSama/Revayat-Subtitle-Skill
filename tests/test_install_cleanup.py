"""Cleanup errors retain complete success or exact transactional failure/cancellation."""
import unittest
from unittest.mock import patch

from check import WorkspaceCase
from test_installation import installer


class InstallCleanupTests(WorkspaceCase):
    def test_cleanup_metadata_does_not_replace_primary_exception(self):
        module = installer()
        for index, primary in enumerate((ValueError("primary"), OSError("primary"), KeyboardInterrupt())):
            with self.subTest(primary=type(primary).__name__):
                target = self.root / f"target-{index}"
                recovery = self.root / f"recovery-{index}"
                real_rmtree = module.shutil.rmtree
                def copy(source, destination):
                    raise primary
                def cleanup(path, *args, **kwargs):
                    if path.name.startswith("new-"):
                        raise PermissionError("cleanup fault")
                    return real_rmtree(path, *args, **kwargs)
                with patch.object(module.shutil, "copyfile", copy), patch.object(module.shutil, "rmtree", cleanup):
                    with self.assertRaises(type(primary)) as caught:
                        module.install(target, plugin=False, force=False, recovery_dir=recovery)
                self.assertIs(caught.exception, primary)
                self.assertFalse(target.exists())

    def test_cleanup_read_failure_after_commit_preserves_success(self):
        module = installer()
        target = self.root / "success"
        original = module.state.safe_path
        def metadata(path, **kwargs):
            if path.name.startswith("new-") and not path.exists():
                raise PermissionError("cleanup metadata")
            return original(path, **kwargs)
        with patch.object(module.state, "safe_path", metadata):
            result = module.install(target, plugin=False, force=False, recovery_dir=self.root / "recovery")
        self.assertIsNone(result)
        self.assertTrue((target / "SKILL.md").is_file())


if __name__ == "__main__":
    unittest.main()
