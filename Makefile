CARGO ?= cargo
PYTHON ?= python
PANDOC ?= pandoc
PREFIX ?= $(if $(CARGO_HOME),$(CARGO_HOME),$(HOME)/.cargo)
MANDIR ?= $(PREFIX)/share/man
DESTDIR ?=

.PHONY: build debug test check fmt install install-man uninstall-man docs docs-check clean help

# Default target: optimized binary at target/release/msg.
build:
	$(CARGO) build --release --locked

debug:
	$(CARGO) build --locked

test:
	$(CARGO) test --locked

check:
	$(CARGO) fmt --check
	$(CARGO) clippy --all-targets --locked -- -D warnings

fmt:
	$(CARGO) fmt

install:
	$(CARGO) install --path . --locked --root "$(PREFIX)"
	$(MAKE) install-man

# Install the checked-in manual; Pandoc is only needed to regenerate it.
install-man:
	install -d "$(DESTDIR)$(MANDIR)/man1"
	install -m 644 man/msg.1 "$(DESTDIR)$(MANDIR)/man1/msg.1"

uninstall-man:
	rm -f "$(DESTDIR)$(MANDIR)/man1/msg.1"

docs:
	$(PYTHON) docs/build.py --pandoc "$(PANDOC)"

docs-check:
	$(PYTHON) docs/build.py --pandoc "$(PANDOC)" --check

clean:
	$(CARGO) clean

help:
	@printf '%s\n' \
	  'make          Build the release binary (target/release/msg)' \
	  'make debug    Build the debug binary (target/debug/msg)' \
	  'make test     Run tests' \
	  'make check    Check formatting and run Clippy' \
	  'make fmt      Format Rust source' \
	  'make install  Install binary and manual under PREFIX (default: Cargo home)' \
	  'make install-man    Install only the prebuilt man page (no Pandoc needed)' \
	  'make uninstall-man  Remove the installed msg man page' \
	  'make docs           Regenerate man/msg.1 and build target/site (needs Pandoc)' \
	  'make docs-check     Build docs and check the committed man page is current' \
	  'make clean    Remove Cargo build artifacts'
