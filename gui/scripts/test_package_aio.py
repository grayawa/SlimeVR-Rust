import hashlib
import importlib.util
import json
import tempfile
import unittest
import zipfile
from pathlib import Path

spec = importlib.util.spec_from_file_location('package_aio', Path(__file__).with_name('package-aio.py'))
aio = importlib.util.module_from_spec(spec)
spec.loader.exec_module(aio)


class AioPackageTests(unittest.TestCase):
    def prepare(self, root):
        for name in aio.BUNDLES:
            path = root / f'{name}.zip'
            with zipfile.ZipFile(path, 'w') as archive:
                archive.writestr('app/readme.txt', name)
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            path.with_suffix('.zip.sha256').write_text(f'{digest}  {path.name}\n')

    def test_preserves_individual_packages_and_records_revision(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.prepare(root)
            result = aio.package(root, 'tested-revision')
            with zipfile.ZipFile(result) as archive:
                self.assertIsNone(archive.testzip())
                manifest = json.loads(archive.read('BUILD-MANIFEST.json'))
                self.assertEqual(manifest['revision'], 'tested-revision')
                self.assertEqual(len(manifest['bundles']), 3)
                for entry in manifest['bundles']:
                    self.assertEqual(archive.read(entry['name']), (root / entry['name']).read_bytes())
            self.assertEqual(result.with_suffix('.zip.sha256').read_text().split()[0], hashlib.sha256(result.read_bytes()).hexdigest())

    def test_modified_bundle_is_rejected_before_creating_collection(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.prepare(root)
            (root / (aio.BUNDLES[0] + '.zip')).write_bytes(b'corrupt')
            with self.assertRaisesRegex(ValueError, 'Checksum mismatch'):
                aio.package(root, 'revision')
            self.assertFalse((root / 'SlimeVR-AIO-Windows-x64.zip').exists())


if __name__ == '__main__':
    unittest.main()
