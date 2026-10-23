# Creating releases

The [Release workflow](../.github/workflows/release.yml) publishes installable
assets to [GitHub Releases](https://github.com/sparkgeo/msg/releases) when a
`v*` tag is pushed. The existing GitHub Pages workflow publishes the manual
independently. This is a download-based release, not an apt repository or a
GitHub Packages registry.

## Included assets

Each release contains four `.tar.gz` archives (Linux and macOS, each for x86-64
and ARM64), two Debian packages (`amd64` and `arm64`), and `SHA256SUMS` covering
all six packages. Archives include `install.sh`, the binary, terminal manual,
full Markdown manual, and example configuration. Packages do not contain
credentials or install a user configuration file.

Linux binaries build natively on Ubuntu 22.04 using glibc. They are not static
musl binaries and do not target Alpine Linux. `dpkg-shlibdeps` derives the Debian
package's minimum library versions from the compiled binary. macOS builds run
on Intel and Apple Silicon runners with `MACOSX_DEPLOYMENT_TARGET=13.0`. They
are not Developer ID signed or notarized; no Apple signing credentials are
configured by this workflow.

Archive names use Rust target triples, for example
`msg-v0.1.0-x86_64-unknown-linux-gnu.tar.gz`. Debian package names use Debian
architecture names and a package revision, for example
`msg_0.1.0-1_amd64.deb`.

## Test without publishing

From the repository's **Actions → Release → Run workflow**, select the branch
to build. The workflow validates the manual, runs formatting, Clippy, and tests,
builds all four native binaries, and checks archive installation on each runner.
On Linux it also installs and removes the `.deb` with apt. Download the combined
`release-bundle` artifact from the run to inspect the packages and checksums.
Manual runs never publish a GitHub Release, even when run against a tag.

Pull requests changing the release workflow, packaging scripts, or installer
run the same build and installation checks without publishing. The repository's
normal CI continues to check application changes.

## Publish a version

1. Update `version` in `Cargo.toml` and run `cargo check` to update `Cargo.lock`.
2. Run `make docs` to regenerate the versioned manual, then `make docs-check`.
3. Commit the version, lockfile, generated manual, and intended release changes.
4. Push the commit and a matching version tag. For example, when `Cargo.toml`
   contains `version = "0.1.0"`:

   ```sh
   git push origin main
   git tag -a v0.1.0 -m 'Release v0.1.0'
   git push origin v0.1.0
   ```

The tag must exactly equal `v` followed by the Cargo package version; a mismatch
fails before platform builds. After every platform succeeds, the workflow
generates checksums and creates a GitHub Release with generated release notes.
Only the publish job has `contents: write`; it uses the built-in `GITHUB_TOKEN`,
so no personal access token is needed. Repository or organization policy must
allow that job permission and the actions used in the workflow.

Versions such as `0.2.0-rc.1` publish as prereleases and use Debian version
`0.2.0~rc.1-1`, which sorts before the stable `0.2.0-1`. A failed build does not
publish a release. Re-running a completed publication does not overwrite an
existing release; create a new version for changed assets. If an upload fails
and leaves a draft release, inspect and remove that incomplete draft before
retrying the publish job.

## Build packages locally

Use Python 3.11+, Rust, and a native host matching one of the workflow targets.
On Debian/Ubuntu, install `dpkg-dev` for dependency discovery and `.deb` creation.
For example, on x86-64 Linux:

```sh
cargo build --release --locked --target x86_64-unknown-linux-gnu
python3 scripts/release.py package --target x86_64-unknown-linux-gnu
python3 scripts/release.py checksums target/packages
python3 -m unittest discover -s scripts -p 'test_*.py'
```

Output goes into `target/packages/`. Use `--binary PATH` to package a different
build location or `--output DIR` to use a fresh output directory. Packaging
verifies that the executable's version matches `Cargo.toml`. On macOS, set
`MACOSX_DEPLOYMENT_TARGET=13.0` before building to match the release workflow.

Archives install under `$HOME/.local` by default; `./install.sh /absolute/prefix`
selects another prefix. Installing replaces the binary/manual at that prefix,
without changing configuration or credential storage. To remove an archive
installation, remove `bin/msg`, `share/man/man1/msg.1`, and `share/doc/msg` under
that prefix. Debian packages install into `/usr` and are removed with
`sudo apt remove msg`.
