#!/bin/sh
# Install a binary archive without requiring Rust or Make.
set -eu

if [ "$#" -gt 1 ]; then
    echo "Usage: $0 [absolute-prefix] (default: \$HOME/.local)" >&2
    exit 2
fi
case "${1:-}" in
    -h|--help)
        echo "Usage: $0 [absolute-prefix] (default: \$HOME/.local)"
        exit 0
        ;;
esac

install_prefix=${1:-${HOME:?HOME is unset; pass an absolute installation prefix}/.local}
case "$install_prefix" in
    /*) ;;
    *) echo "Installation prefix must be an absolute path" >&2; exit 2 ;;
esac
archive_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

install -d "$install_prefix/bin" "$install_prefix/share/man/man1" "$install_prefix/share/doc/msg"
install -m 755 "$archive_dir/bin/msg" "$install_prefix/bin/msg"
install -m 644 "$archive_dir/share/man/man1/msg.1" "$install_prefix/share/man/man1/msg.1"
for document in README.md manual.md config.example.toml; do
    install -m 644 "$archive_dir/share/doc/msg/$document" "$install_prefix/share/doc/msg/$document"
done
printf 'Installed msg to %s/bin/msg\nKeep %s/bin on PATH.\n' "$install_prefix" "$install_prefix"
