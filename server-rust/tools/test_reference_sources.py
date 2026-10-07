import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

import reference_sources


class ReferenceSourcesTests(unittest.TestCase):
    def test_reference_export_survives_removal_of_active_java_sources(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "project"
            source = root / "server/core/src/main/java/Reference.kt"
            source.parent.mkdir(parents=True)
            source.write_bytes(b"pinned reference\n")
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            subprocess.run(["git", "-c", "core.autocrlf=false", "add", "."], cwd=root, check=True)
            subprocess.run(
                ["git", "-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                 "commit", "-qm", "Reference snapshot"], cwd=root, check=True,
            )
            revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
            shutil.rmtree(root / "server")
            with mock.patch.object(reference_sources, "REFERENCE_COMMIT", revision), \
                 mock.patch.object(reference_sources.tempfile, "gettempdir", return_value=directory), \
                 mock.patch.dict(os.environ, {"SLIMEVR_REFERENCE_ROOT": ""}):
                reference, actual_revision = reference_sources.reference_checkout(root)
                archived = reference / source.relative_to(root)
                self.assertEqual(archived.read_bytes(), b"pinned reference\n")
                self.assertEqual(actual_revision, revision)
                self.assertFalse((root / "server").exists())
                self.assertEqual(reference_sources.source_path(archived, root, reference), "server/core/src/main/java/Reference.kt")
                self.assertEqual(reference_sources.reference_checkout(root), (reference, revision))

    def test_invalid_explicit_reference_is_reported(self):
        with tempfile.TemporaryDirectory() as directory, \
             mock.patch.dict(os.environ, {"SLIMEVR_REFERENCE_ROOT": directory}):
            with self.assertRaisesRegex(ValueError, "upstream SlimeVR checkout"):
                reference_sources.reference_checkout(Path(directory))


if __name__ == "__main__":
    unittest.main()
