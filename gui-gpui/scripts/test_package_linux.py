import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import tarfile
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("package_linux", Path(__file__).with_name("package-linux.py"))
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


def elf(path, machine=62):
    path.parent.mkdir(parents=True, exist_ok=True)
    data = bytearray(64)
    data[:6] = b"\x7fELF\x02\x01"
    struct.pack_into("<H", data, 18, machine)
    path.write_bytes(data)
    return path


class LinuxPackage(unittest.TestCase):
    def fixture(self, directory):
        root = Path(directory)
        driver = root / "drivers/slimevr-openvr-driver-x64-linux"
        bindings = root / "bindings/linux64"
        elf(driver / "bin/linux64/driver_slimevr.so")
        (driver / "driver.vrdrivermanifest").write_text('{"name":"slimevr"}')
        for name in ["LICENSE-MIT", "LICENSE-APACHE"]:
            (driver.parent / name).write_text("driver license fixture")
        elf(bindings / "libopenvr_api.so")
        elf(bindings / "slimevr-bindings-provider")
        (bindings.parent / "OPENVR-LICENSE").write_text("OpenVR license fixture")
        return argparse.Namespace(gui=elf(root / "gui"), server=elf(root / "server"),
                                  components=elf(root / "components"), probe=elf(root / "probe"),
                                  driver_dir=driver, bindings_dir=bindings, arch="x64",
                                  build_revision="a" * 40, audit_system_libs=False,
                                  output=root / "output/package.tar.gz")

    def test_distribution_contract(self):
        with tempfile.TemporaryDirectory() as directory:
            args = self.fixture(directory)
            package.build_package(args)
            with tarfile.open(args.output) as archive:
                prefix = "SlimeVR-Rust-GPUI/"
                manifest = json.load(archive.extractfile(prefix + "BUILD-MANIFEST.json"))
                self.assertEqual(manifest["target"], "Linux x64")
                self.assertEqual(manifest["build_revision"], "a" * 40)
                for entry in manifest["files"]:
                    member = archive.getmember(prefix + entry["path"])
                    content = archive.extractfile(member).read()
                    self.assertEqual(hashlib.sha256(content).hexdigest(), entry["sha256"])
                    self.assertEqual(len(content), entry["bytes"])
                    self.assertEqual(oct(member.mode), entry["mode"])
                for name in ["slimevr-gpui", "slimevr-server", "slimevr-gpui-components", "Start-SlimeVR.sh",
                             "bindings/linux64/slimevr-bindings-provider"]:
                    self.assertEqual(archive.getmember(prefix + name).mode, 0o755)
                for name in ["LICENSE", "SOURCE-CODE.txt", "licenses/ksni-UNLICENSE", "licenses/OpenVR-LICENSE",
                             "drivers/LICENSE-MIT", "drivers/LICENSE-APACHE", "69-slimevr-devices.rules"]:
                    self.assertTrue(archive.getmember(prefix + name).isfile())
                source = archive.extractfile(prefix + "SOURCE-CODE.txt").read().decode()
                self.assertIn("/tree/" + "a" * 40, source)
            self.assertEqual(args.output.with_suffix(".gz.sha256").read_text().split()[0],
                             hashlib.sha256(args.output.read_bytes()).hexdigest())

    def test_rejects_mixed_architectures(self):
        with tempfile.TemporaryDirectory() as directory:
            args = self.fixture(directory)
            elf(args.bindings_dir / "libopenvr_api.so", 183)
            with self.assertRaisesRegex(ValueError, "architecture mismatch"):
                package.build_package(args)
            self.assertFalse(args.output.exists())

    def test_requires_driver_and_license_resources(self):
        with tempfile.TemporaryDirectory() as directory:
            args = self.fixture(directory)
            (args.driver_dir / "driver.vrdrivermanifest").unlink()
            with self.assertRaises(FileNotFoundError):
                package.build_package(args)
            self.assertFalse(args.output.exists())

    def test_resource_symlinks_are_checked(self):
        with tempfile.TemporaryDirectory() as directory:
            args = self.fixture(directory)
            (args.bindings_dir / "outside").symlink_to(args.gui)
            with self.assertRaisesRegex(ValueError, "symlink"):
                package.build_package(args)
            self.assertFalse(args.output.exists())


if __name__ == "__main__":
    unittest.main()
