# Install msg

This archive contains a compiled binary, `man msg`, and example configuration.
Rust and Make are not required.

From the extracted directory, install under your home directory:

```sh
./install.sh
export PATH="$HOME/.local/bin:$PATH"
msg --version
man msg
```

Or choose another absolute prefix:

```sh
sudo ./install.sh /usr/local
```

Installation replaces an existing binary/manual at that prefix. Configuration
and credentials are not created or changed. If your manual viewer does not
search the prefix automatically, add its `share/man` directory to `MANPATH`,
preserving the default search path with a trailing colon.

Linux archives are built on Ubuntu 22.04 with glibc (not Alpine/musl).
macOS archives target macOS 13 or newer; choose `aarch64` for Apple Silicon
or `x86_64` for Intel. macOS binaries are not Developer ID signed or notarized.

On Debian/Ubuntu, the matching `.deb` release asset is an alternative:

```sh
sudo apt install ./msg_VERSION_ARCH.deb
```

It installs into `/usr` and declares the binary's shared-library dependencies.
Use `sudo apt remove msg` to uninstall a Debian package. To uninstall an archive
installation, remove `bin/msg`, `share/man/man1/msg.1`, and `share/doc/msg` under
the prefix you selected. Your configuration and credentials remain separate.

Releases and checksums: https://github.com/sparkgeo/msg/releases

See `share/doc/msg/manual.md` or `man msg` for account setup and usage.
