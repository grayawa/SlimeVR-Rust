import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('notices', Path(__file__).with_name('distribution-notices.py'))
notices = importlib.util.module_from_spec(spec)
spec.loader.exec_module(notices)


class DistributionNotices(unittest.TestCase):
    def make_root(self, root):
        for name in notices.PROJECT_FILES:
            (root / name).write_text('original ' + name)
        fonts = root / 'gui-gpui/assets/fonts'
        fonts.mkdir(parents=True)
        (fonts / 'Font-OFL.txt').write_text('font copyright and license')
        (fonts / 'font.ttf').write_bytes(b'not a license')

    def test_refreshing_a_baseline_keeps_vendor_notices_and_adds_exact_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / 'root'
            destination = Path(directory) / 'bundle'
            root.mkdir()
            self.make_root(root)
            vendor = destination / 'licenses/vendor/NOTICE'
            vendor.parent.mkdir(parents=True)
            vendor.write_text('keep original vendor copyright')
            (destination / 'NOTICE').write_text('stale project notice')
            notices.copy_notices(root, destination, 'a' * 40)
            self.assertEqual(vendor.read_text(), 'keep original vendor copyright')
            for name in notices.PROJECT_FILES:
                self.assertEqual((destination / name).read_bytes(), (root / name).read_bytes())
            self.assertIn('/tree/' + 'a' * 40, (destination / 'SOURCE-CODE.txt').read_text())
            self.assertTrue((destination / 'licenses/fonts/Font-OFL.txt').is_file())
            self.assertFalse((destination / 'licenses/fonts/font.ttf').exists())

    def test_missing_project_license_fails_before_overwriting_the_baseline(self):
        with tempfile.TemporaryDirectory() as directory:
            root, destination = Path(directory) / 'root', Path(directory) / 'bundle'
            root.mkdir()
            destination.mkdir()
            self.make_root(root)
            (root / 'LICENSE').unlink()
            (destination / 'NOTICE').write_text('existing notice')
            with self.assertRaises(ValueError):
                notices.copy_notices(root, destination, 'a' * 40)
            self.assertEqual((destination / 'NOTICE').read_text(), 'existing notice')
            self.assertFalse((destination / 'SOURCE-CODE.txt').exists())

    def test_local_label_does_not_claim_a_nonexistent_source_commit(self):
        with tempfile.TemporaryDirectory() as directory:
            root, destination = Path(directory) / 'root', Path(directory) / 'bundle'
            root.mkdir()
            self.make_root(root)
            notices.copy_notices(root, destination, 'dashboard')
            source = (destination / 'SOURCE-CODE.txt').read_text()
            self.assertNotIn('/tree/dashboard', source)
            self.assertIn('No exact source commit recorded', source)

    def test_fork_builds_reference_the_forks_corresponding_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root, destination = Path(directory) / 'root', Path(directory) / 'bundle'
            root.mkdir()
            self.make_root(root)
            with patch.dict(os.environ, {'GITHUB_REPOSITORY': 'example/fork'}):
                notices.copy_notices(root, destination, 'b' * 40)
            self.assertIn('https://github.com/example/fork/tree/' + 'b' * 40,
                          (destination / 'SOURCE-CODE.txt').read_text())


if __name__ == '__main__':
    unittest.main()
