# Maintaining the manual

For downloadable binaries and Debian packages, see [creating releases](releases.md).

Edit [manual.md](manual.md), the source for both the website and `man msg`.
The generated [man page](../man/msg.1) is checked in so installation needs
neither Python nor Pandoc. Do not edit the generated roff directly.

## Build and preview

Use Python 3.11+ and [Pandoc 3.7.0.2](https://github.com/jgm/pandoc/releases/tag/3.7.0.2)
(the version pinned in CI). Pandoc is a separate program; Cargo and Python
do not install it. Confirm it is available with `pandoc --version`.

On Ubuntu/Debian, install it with:

```sh
sudo apt-get install pandoc
pandoc --version
```

Distribution packages may provide a different version. For reproducible
output matching CI, use the 3.7.0.2 release linked above. Put its executable
on PATH, or supply `PANDOC=/path/to/pandoc` to Make. Then run:

```sh
make docs
make docs-check
man ./man/msg.1
python3 -m http.server --directory target/site 8000
```

Open <http://localhost:8000>. The website has a table of contents, light/dark
styles, printable pages, and downloads of the man page, source, and example
configuration. All assets and internal links are relative so it works under
a GitHub project URL or custom domain, and the site has no external runtime
dependencies. Browser Find searches the whole manual.

`make docs` regenerates `man/msg.1` and builds `target/site/`. Commit the
source and generated man page together. `make docs-check` fails if the man
page differs from the source, then builds and checks the website's local
links. Run it when changing the manual, template, stylesheet, example config,
or application version. Changing the generator version can change roff
formatting; update the workflow pin and regenerate together.

Override tools with `make docs PYTHON=python3 PANDOC=/path/to/pandoc`.

## Install the terminal manual

```sh
make install                              # binary + prebuilt manual
make install-man                          # manual only
make install PREFIX="$HOME/.local"        # bin/ and share/man/ in this prefix
make install-man MANDIR=/usr/local/share/man
make uninstall-man                        # remove only the manual
```

The default `PREFIX` is `CARGO_HOME` if nonempty, otherwise `$HOME/.cargo`.
`MANDIR` defaults to `PREFIX/share/man`. The man viewer can usually infer
that directory from `PREFIX/bin` in PATH. If it cannot, add the appropriate
directory to MANPATH, preserving default paths with a trailing colon:

```sh
export MANPATH="$HOME/.cargo/share/man:${MANPATH:-}"
man msg
```

`cargo install` by itself installs binaries only. Follow it with
`make install-man` if using that installation method. Cargo's other install
root settings are not consulted by Make; supply `PREFIX` explicitly when
using a custom Cargo install root.

Packagers can stage the manual with
`make install-man DESTDIR=/tmp/package PREFIX=/usr`; this writes
`/tmp/package/usr/share/man/man1/msg.1`. `DESTDIR` applies only to the manual
targets, not Cargo's binary installation. No privileged install, shell
startup changes, or manual-database update is performed automatically.

## Publish with GitHub Actions

1. Push this repository to GitHub, with `main` as the publishing branch.
2. In **Settings → Pages → Build and deployment**, select **GitHub Actions**
   as the source.
3. Push to `main`, or run the **Manual** workflow manually from Actions.
4. Open the URL shown by the `github-pages` deployment.

The [Manual workflow](../.github/workflows/docs.yml) builds and checks both
formats on pull requests. Publishing is limited to pushes/manual runs on
`main`, with Pages and OIDC write permissions only in the deployment job.
Pull requests do not deploy. If your default branch has another name,
update the workflow's branch filter and both deployment conditions. Follow
GitHub's [custom Pages workflow instructions](https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages)
for repository setup and environment protection settings.

No repository URL is hard-coded, so forks can publish their own manual.
If Pages is not enabled yet, the build can pass while the deployment fails;
enable it and rerun the workflow. Locally, `target/site/index.html` is ready
to preview without enabling GitHub hosting.
