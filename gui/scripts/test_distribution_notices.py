import importlib.util
import os
import hashlib
import json
import re
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from urllib.parse import unquote, urlsplit

spec = importlib.util.spec_from_file_location('notices', Path(__file__).with_name('distribution-notices.py'))
notices = importlib.util.module_from_spec(spec)
spec.loader.exec_module(notices)

stage_spec = importlib.util.spec_from_file_location(
    'tauri_notices', Path(__file__).with_name('prepare-tauri-notices.py'))
tauri_notices = importlib.util.module_from_spec(stage_spec)
stage_spec.loader.exec_module(tauri_notices)


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


class DistributionDocumentation(unittest.TestCase):
    def test_bundle_navigation_and_source_links_match_the_build(self):
        root = Path(__file__).resolve().parents[2]
        with tempfile.TemporaryDirectory() as work:
            destination = Path(work)
            revision = 'c' * 40
            with patch.dict(os.environ, {'GITHUB_REPOSITORY': 'example/fork'}):
                notices.copy_notices(root, destination, revision)
                notices.copy_documentation(root, destination, revision, [
                    ('gui-gpui/README.zh-CN.md', '原生前端说明.md'),
                    ('docs/rust-steamvr-dashboard.zh-CN.md', '面板说明与测试.md')])
            for name in ['README.md', 'CHANGELOG.md', 'docs/README.md',
                         'docs/rust-feature-status.zh-CN.md', '原生前端说明.md']:
                self.assertTrue((destination / name).is_file())
            for path in destination.rglob('*.md'):
                if path.relative_to(destination).as_posix() in notices.PROJECT_FILES or 'licenses' in path.parts:
                    continue
                for href in re.findall(r'!?\[[^\]\n]*\]\(([^\s)]+)\)', path.read_text()):
                    url = urlsplit(href)
                    if not url.scheme and not url.netloc and url.path:
                        target = path.parent.joinpath(unquote(url.path)).resolve()
                        self.assertTrue(target.is_relative_to(destination), (path, href))
                        self.assertTrue(target.exists(), (path, href))
            source_link = f'https://github.com/example/fork/blob/{revision}/'
            guide = (destination / 'docs/rust-steamvr-bridge.zh-CN.md').read_text()
            self.assertIn(source_link + 'server-rust/crates/slimevr-core/src/velocity.rs', guide)
            self.assertIn('(rust-unified-hardware-test.zh-CN.md#steamvr)', guide)
            native = (destination / '原生前端说明.md').read_text()
            self.assertIn('(docs/rust-gpui-guide.zh-CN.md)', native)
            self.assertIn('cargo build --manifest-path gui-gpui/Cargo.toml', native)
            for name in notices.PROJECT_FILES:
                self.assertEqual((destination / name).read_bytes(), (root / name).read_bytes())

    def test_missing_guide_is_reported_before_copying(self):
        with tempfile.TemporaryDirectory() as work:
            root, destination = Path(work) / 'root', Path(work) / 'bundle'
            root.mkdir()
            (root / 'README.md').write_text('Project guide')
            with self.assertRaisesRegex(ValueError, 'CHANGELOG.md'):
                notices.copy_documentation(root, destination, 'local-preview')
            self.assertFalse(destination.exists())


class TauriInstallerNotices(unittest.TestCase):
    def make_checkout(self, root):
        DistributionNotices().make_root(root)
        for name in ('server-rust/Cargo.lock', 'gui/src-tauri/Cargo.lock', 'pnpm-lock.yaml'):
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('locked build inputs\n')
        (root / '.gitignore').write_text('/gui/src-tauri/resources/\n')
        subprocess.run(['git', 'init', '-q', str(root)], check=True)
        subprocess.run(['git', 'add', '.'], cwd=root, check=True)
        subprocess.run(['git', '-c', 'user.name=Fixture',
                        '-c', 'user.email=fixture@example.invalid',
                        '-c', 'commit.gpgsign=false', 'commit', '-qm', 'fixture',
                        '--no-verify'], cwd=root, check=True)
        return subprocess.check_output(
            ['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()

    def test_installer_carries_exact_licenses_and_locked_source_references(self):
        with tempfile.TemporaryDirectory() as work:
            root = Path(work)
            commit = self.make_checkout(root)
            destination = tauri_notices.prepare(root)
            for name in notices.PROJECT_FILES:
                self.assertEqual((destination / name).read_bytes(), (root / name).read_bytes())
            self.assertIn('/tree/' + commit, (destination / 'SOURCE-CODE.txt').read_text())
            manifest = json.loads((destination / 'BUILD-SOURCE.json').read_text())
            self.assertEqual(manifest['commit'], commit)
            self.assertFalse(manifest['local_changes'])
            for name, digest in manifest['lockfile_sha256'].items():
                self.assertEqual(digest, hashlib.sha256((root / name).read_bytes()).hexdigest())
            self.assertTrue((destination / 'licenses/fonts/Font-OFL.txt').is_file())
            self.assertFalse((destination / 'licenses/fonts/font.ttf').exists())

    def test_local_changes_are_recorded_against_the_checkout_commit(self):
        with tempfile.TemporaryDirectory() as work:
            root = Path(work)
            commit = self.make_checkout(root)
            (root / 'LICENSE').write_text('updated project license')
            manifest = json.loads((tauri_notices.prepare(root) / 'BUILD-SOURCE.json').read_text())
            self.assertEqual(manifest['commit'], commit)
            self.assertTrue(manifest['local_changes'])

    def test_refresh_uses_only_the_current_license_tree(self):
        with tempfile.TemporaryDirectory() as work:
            root = Path(work)
            self.make_checkout(root)
            destination = tauri_notices.prepare(root)
            (destination / 'stale.txt').write_text('previous build')
            tauri_notices.prepare(root)
            self.assertFalse((destination / 'stale.txt').exists())

    def test_incomplete_inputs_preserve_the_existing_staged_bundle(self):
        with tempfile.TemporaryDirectory() as work:
            root = Path(work)
            self.make_checkout(root)
            destination = tauri_notices.prepare(root)
            original = (destination / 'SOURCE-CODE.txt').read_bytes()
            (root / 'LICENSE').unlink()
            with self.assertRaises(ValueError):
                tauri_notices.prepare(root)
            self.assertEqual((destination / 'SOURCE-CODE.txt').read_bytes(), original)

    def test_tauri_build_hooks_stage_notices_for_all_platforms(self):
        root = Path(__file__).resolve().parents[2]
        config = json.loads((root / 'gui/src-tauri/tauri.conf.json').read_text())
        scripts = json.loads((root / 'gui/package.json').read_text())['scripts']
        for hook in ('beforeDevCommand', 'beforeBuildCommand'):
            self.assertTrue(config['build'][hook].startswith('pnpm tauri:notices && '))
        self.assertIn('prepare-tauri-notices.py', scripts['tauri:notices'])
        self.assertEqual(config['bundle']['resources']['resources/notices/'], '')


if __name__ == '__main__':
    unittest.main()
