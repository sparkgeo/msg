#!/usr/bin/env python3
"""Package native msg binaries for GitHub Releases. Requires Python 3.11+."""

import argparse
import gzip
import hashlib
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGETS = {
    "x86_64-unknown-linux-gnu": "amd64",
    "aarch64-unknown-linux-gnu": "arm64",
    "x86_64-apple-darwin": None,
    "aarch64-apple-darwin": None,
}


def version(tag=""):
    value = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", value):
        raise ValueError("Cargo.toml must contain a semantic version")
    if tag and tag != f"v{value}":
        raise ValueError(f"release tag {tag!r} must match Cargo.toml version v{value}")
    return value


def debian_version(value):
    # Debian's ~ sorts prereleases before the corresponding stable version.
    upstream, separator, metadata = value.partition("+")
    return upstream.replace("-", "~", 1) + separator + metadata + "-1"


def copy_file(source, destination, mode=0o644):
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    destination.chmod(mode)


def stage_payload(directory, binary):
    copy_file(binary, directory / "bin/msg", 0o755)
    copy_file(ROOT / "man/msg.1", directory / "share/man/man1/msg.1")
    for source, name in [
        (ROOT / "packaging/INSTALL.md", "README.md"),
        (ROOT / "docs/manual.md", "manual.md"),
        (ROOT / "config.example.toml", "config.example.toml"),
    ]:
        copy_file(source, directory / "share/doc/msg" / name)


def archive_metadata(info):
    info.uid = info.gid = 0
    info.uname = info.gname = "root"
    info.mtime = 0
    if info.isdir():
        info.mode = 0o755
    return info


def build_archive(binary, target, value, output):
    name = f"msg-v{value}-{target}"
    destination = output / f"{name}.tar.gz"
    with tempfile.TemporaryDirectory() as temporary:
        payload = Path(temporary) / name
        stage_payload(payload, binary)
        copy_file(ROOT / "packaging/install.sh", payload / "install.sh", 0o755)
        copy_file(ROOT / "packaging/INSTALL.md", payload / "INSTALL.md")
        with destination.open("wb") as stream:
            with gzip.GzipFile(filename="", fileobj=stream, mode="wb", mtime=0) as compressed:
                with tarfile.open(fileobj=compressed, mode="w") as archive:
                    archive.add(payload, arcname=name, filter=archive_metadata)
    return destination


def build_deb(binary, architecture, value, output):
    package_version = debian_version(value)
    destination = output / f"msg_{package_version}_{architecture}.deb"
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary)
        payload = workspace / "debian/msg"
        stage_payload(payload / "usr", binary)
        control = payload / "DEBIAN"
        control.mkdir()
        # dpkg-shlibdeps reads the build machine's package metadata to determine
        # the actual minimum library versions required by this binary.
        (workspace / "debian").mkdir(exist_ok=True)
        (workspace / "debian/control").write_text(
            "Source: msg\nSection: utils\nPriority: optional\n"
            "Maintainer: msg contributors\n\n"
            "Package: msg\nArchitecture: any\nDescription: Send terminal messages\n"
        )
        dependencies = subprocess.check_output(
            ["dpkg-shlibdeps", "-O", "-edebian/msg/usr/bin/msg"], cwd=workspace, text=True
        )
        depends = next(
            (line.removeprefix("shlibs:Depends=") for line in dependencies.splitlines()
             if line.startswith("shlibs:Depends=")), ""
        )
        if not depends:
            raise ValueError("dpkg-shlibdeps did not report runtime dependencies")
        size = sum(path.stat().st_size for path in payload.rglob("*") if path.is_file())
        (control / "control").write_text(
            f"Package: msg\nVersion: {package_version}\nArchitecture: {architecture}\n"
            "Section: utils\nPriority: optional\nMaintainer: msg contributors\n"
            f"Depends: {depends}\nInstalled-Size: {(size + 1023) // 1024}\n"
            "Homepage: https://github.com/sparkgeo/msg\n"
            "Description: Send one message to stdout and Slack\n"
            " A synchronous command-line messenger with configurable credential sources.\n"
        )
        man = payload / "usr/share/man/man1/msg.1"
        with man.with_suffix(".1.gz").open("wb") as stream:
            with gzip.GzipFile(filename="", fileobj=stream, mode="wb", mtime=0) as compressed:
                compressed.write(man.read_bytes())
        man.unlink()
        for path in [payload, *payload.rglob("*")]:
            path.chmod(0o755 if path.is_dir() or path == payload / "usr/bin/msg" else 0o644)
        subprocess.run(
            ["dpkg-deb", "--root-owner-group", "--build", str(payload), str(destination)],
            check=True,
        )
    return destination


def checksums(directory):
    files = sorted(path for path in directory.iterdir() if path.name.endswith((".tar.gz", ".deb")))
    if not files:
        raise ValueError("no release packages found")
    lines = []
    for path in files:
        with path.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        lines.append(f"{digest}  {path.name}\n")
    (directory / "SHA256SUMS").write_text("".join(lines))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    check = commands.add_parser("version", help="check Cargo version and optionally a release tag")
    check.add_argument("--tag", default="")
    package = commands.add_parser("package", help="package an already-built native binary")
    package.add_argument("--target", required=True, choices=TARGETS)
    package.add_argument("--binary", type=Path, help="default: target/TARGET/release/msg")
    package.add_argument("--output", type=Path, default=ROOT / "target/packages")
    checksum = commands.add_parser("checksums", help="write SHA256SUMS for a package directory")
    checksum.add_argument("directory", type=Path)
    args = parser.parse_args()
    if args.command == "version":
        print(version(args.tag))
    elif args.command == "checksums":
        checksums(args.directory)
    else:
        binary = (args.binary or ROOT / "target" / args.target / "release/msg").resolve()
        value = version()
        actual = subprocess.check_output([str(binary), "--version"], text=True).strip()
        if actual != f"msg {value}":
            raise ValueError(f"binary version {actual!r} does not match Cargo.toml")
        host = subprocess.check_output(["rustc", "-vV"], text=True)
        if f"host: {args.target}\n" not in host:
            raise ValueError("packaging must run on the target's native Rust host")
        output = args.output.resolve()
        output.mkdir(parents=True, exist_ok=True)
        print(build_archive(binary, args.target, value, output))
        if architecture := TARGETS[args.target]:
            print(build_deb(binary, architecture, value, output))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(f"release: {error}")
