"""Regression checks for release naming, archive contents, and installation."""

import hashlib
import os
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import release


class ReleaseTests(unittest.TestCase):
    def test_tag_must_match_cargo_version(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text('[package]\nversion = "1.2.3-rc.1"\n')
            with patch.object(release, "ROOT", root):
                self.assertEqual(release.version(), "1.2.3-rc.1")
                self.assertEqual(release.version("v1.2.3-rc.1"), "1.2.3-rc.1")
                for tag in ["v1.2.3", "1.2.3-rc.1", "v9.0.0"]:
                    with self.assertRaisesRegex(ValueError, "must match"):
                        release.version(tag)

    def test_debian_versions_sort_prereleases_before_stable(self):
        self.assertEqual(release.debian_version("1.2.3"), "1.2.3-1")
        self.assertEqual(release.debian_version("1.2.3-rc.1"), "1.2.3~rc.1-1")
        self.assertEqual(release.debian_version("1.2.3-rc.1+build.4"), "1.2.3~rc.1+build.4-1")

    def test_archive_installs_binary_manual_and_example_with_spaces_in_paths(self):
        with tempfile.TemporaryDirectory(prefix="msg release ") as temporary:
            directory = Path(temporary)
            binary = directory / "msg"
            binary.write_text("#!/bin/sh\nprintf 'msg 1.2.3\\n'\n")
            binary.chmod(0o755)
            archive_path = release.build_archive(
                binary, "x86_64-unknown-linux-gnu", "1.2.3", directory
            )
            with tarfile.open(archive_path) as archive:
                for member in archive.getmembers():
                    self.assertEqual(member.uid, 0)
                    self.assertEqual(member.gid, 0)
                    self.assertFalse(member.name.startswith("/"))
                archive.extractall(directory, filter="data")
            extracted = directory / "msg-v1.2.3-x86_64-unknown-linux-gnu"
            prefix = directory / "installed prefix"
            subprocess.run([str(extracted / "install.sh"), str(prefix)], check=True, capture_output=True)
            self.assertEqual(
                subprocess.check_output([str(prefix / "bin/msg"), "--version"], text=True),
                "msg 1.2.3\n",
            )
            self.assertEqual((prefix / "bin/msg").stat().st_mode & 0o777, 0o755)
            self.assertEqual(
                (prefix / "share/man/man1/msg.1").read_bytes(),
                (release.ROOT / "man/msg.1").read_bytes(),
            )
            self.assertTrue((prefix / "share/doc/msg/config.example.toml").is_file())
            self.assertFalse((prefix / "etc").exists())

            # Reinstallation must not create or alter user configuration.
            home = directory / "home"
            config = home / ".config/msg/config.toml"
            config.parent.mkdir(parents=True)
            config.write_text("# user configuration\n")
            environment = {**os.environ, "HOME": str(home)}
            subprocess.run([str(extracted / "install.sh")], env=environment, check=True, capture_output=True)
            self.assertTrue((home / ".local/bin/msg").is_file())
            self.assertEqual(config.read_text(), "# user configuration\n")
            result = subprocess.run(
                [str(extracted / "install.sh"), "relative-prefix"], cwd=directory,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 2)
            self.assertFalse((directory / "relative-prefix").exists())

    def test_checksums_cover_only_release_packages_and_are_repeatable(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for name in ["msg-v1.2.3-target.tar.gz", "msg_1.2.3-1_amd64.deb", "other.txt"]:
                (directory / name).write_bytes(name.encode())
            release.checksums(directory)
            first = (directory / "SHA256SUMS").read_text()
            self.assertEqual(len(first.splitlines()), 2)
            for line in first.splitlines():
                digest, name = line.split("  ")
                self.assertEqual(digest, hashlib.sha256((directory / name).read_bytes()).hexdigest())
            release.checksums(directory)
            self.assertEqual((directory / "SHA256SUMS").read_text(), first)
            self.assertNotIn("other.txt", first)

    def test_checksums_reject_empty_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(ValueError, "no release packages"):
                release.checksums(Path(temporary))


if __name__ == "__main__":
    unittest.main()
