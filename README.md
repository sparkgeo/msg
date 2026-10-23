# msg

Send one text message to stdout, Slack, or several destinations from Linux or macOS. Read from an argument or stdin, send synchronously, and exit with a result usable by scripts.

This first iteration supports Slack bot tokens and stdout. Email, incoming webhooks, send-and-forget, queues, and streaming are not implemented.

## Complete manual

Read the [complete msg manual](docs/manual.md) for every command-line option,
configuration field, credential source, exit status, and practical examples.
The same manual is available in the terminal:

```bash
make install   # Install the binary and manual
man msg

# Or read from the checkout without installing:
man ./man/msg.1
```

The [documentation guide](docs/README.md) explains local previews, installation
paths, and publishing the web manual to GitHub Pages using the included
GitHub Actions workflow.

## Build and install

### Install a release

The [GitHub Releases](https://github.com/sparkgeo/msg/releases) workflow produces
prebuilt packages, so installing a release does not require Rust:

| Platform | Downloads |
| --- | --- |
| Linux x86-64 | `x86_64-unknown-linux-gnu.tar.gz` archive or `amd64.deb` package |
| Linux ARM64 | `aarch64-unknown-linux-gnu.tar.gz` archive or `arm64.deb` package |
| macOS Intel | `x86_64-apple-darwin.tar.gz` archive |
| macOS Apple Silicon | `aarch64-apple-darwin.tar.gz` archive |

Asset names also include the release version. Download the matching asset and
`SHA256SUMS`. Check its checksum with `sha256sum --check --ignore-missing SHA256SUMS`
on Linux; on macOS, compare `shasum -a 256 FILE` with the matching line in
`SHA256SUMS`.

For Debian/Ubuntu, install the downloaded package (example for version 0.1.0):

```sh
sudo apt install ./msg_0.1.0-1_amd64.deb
```

For an archive, extract it, enter the extracted directory, and run:

```sh
./install.sh                       # Install under $HOME/.local
export PATH="$HOME/.local/bin:$PATH"
msg --version
man msg
```

Use `sudo ./install.sh /usr/local` for a system installation. Archives include
the binary, manual, and example config. Linux archives use glibc and are built
on Ubuntu 22.04; macOS binaries target macOS 13+ and are not Developer ID signed
or notarized. The `.deb` declares its actual shared-library dependencies.

See [creating releases](docs/releases.md) for the tag-triggered workflow and
how to test packages without publishing.

### Build from source

Requires Rust 1.88 or newer and a native compiler toolchain. Linux uses a Rust D-Bus client and Rust cryptography; OpenSSL and libdbus development packages are not required. macOS uses the system Keychain framework.

```bash
cargo build --release --locked
./target/release/msg --help

# Optional: install into Cargo's bin directory.
cargo install --path . --locked
```

Alternatively, use the Makefile:

```bash
make           # Build target/release/msg
make debug     # Build target/debug/msg
make check     # Check formatting and run Clippy
make test      # Run tests
make install   # Install binary and manual under Cargo home (or PREFIX)
make install-man # Install only the prebuilt man page
make docs      # Build web/manual docs (Python 3.11+ and Pandoc 3.7.0.2)
make clean     # Remove build artifacts
```

Run `make help` for all targets. Override Cargo if needed, for example `make CARGO=/path/to/cargo`.
Use `make install PREFIX="$HOME/.local"` to select another installation prefix.
Direct `cargo install` installs only the binary; follow it with `make install-man`
to add `man msg`. If your manual viewer cannot find it, see the
[manual installation instructions](docs/README.md#install-the-terminal-manual).

## Send and pipe

```bash
msg 'Hello'
msg Hello world
msg --no-newline Hello world
printf 'Hello\n' | msg
msg --to 'slack:#team' 'Deployment complete'
msg --to 'slack:#team' --to 'slack:#operations' 'Deployment complete'

# Address Slack directly using the stored account token.
msg --to 'slack:#my-channel' 'Hello'
msg --to 'slack:@dustin' 'Hello'

# Send to Slack and pass the entire original content to the next command.
generate-report | msg --to 'slack:#team' --passthrough | save-report

# Explicit stdout is equivalent to --passthrough.
generate-report | msg --to 'slack:#team' --to stdout > report.txt

# Limit the Slack message to 500 Unicode characters; stdout stays complete.
generate-report | msg --to 'slack:#team' -n 500 --passthrough > report.txt
```

`generate-report` and `save-report` above stand for your own commands.

Use `--no-newline` to suppress the newline appended to argument messages on stdout, like `echo -n`. It preserves newlines already present in the message or stdin and does not affect Slack.

`--to` flags are the only way to select remote recipients. Without `--to`, the output is always stdout. Configuration stores accounts and application settings, never recipient locations. `--passthrough` adds stdout to the selected outputs; stdout is not added automatically just because output is redirected. This avoids printing messages unexpectedly in cron or service logs. Repeated identical destination strings are delivered once, preserving the first occurrence; `--passthrough` puts stdout first. Different spellings (for example a channel name and its ID) that resolve to the same recipient are separate deliveries.

Argument messages behave like basic `echo`: words are joined with single spaces and stdout appends a newline, even if the message already ends with one. Quoted whitespace and literal backslashes are preserved. The added newline is only for stdout; Slack receives the joined message. Piped or redirected stdin is written to stdout exactly as received, including whitespace, CRLF, and trailing newlines. `-n` retains its Slack length-limit meaning (it does not suppress the stdout newline) and counts Unicode scalar values, not bytes or grapheme clusters. No ellipsis is added. Slack text is escaped to avoid interpreting mentions, markup, and links; previews are disabled.

The app buffers stdin until EOF, with an 8 MiB limit. Empty input and invalid UTF-8 are rejected; whitespace-only messages are allowed, subject to Slack's own acceptance rules. An explicit message plus nonempty stdin is rejected. An empty redirected stdin (for example `/dev/null` in a service) is allowed with an argument. Reading an ongoing stream such as `tail -f` waits for EOF. Use `--` before text that looks like a flag or subcommand.

To detect failures anywhere in a Bash pipeline, enable `set -o pipefail`; otherwise the shell normally reports only the last command's exit status. `msg` cannot determine whether its upstream producer succeeded.

## Configuration

The default file is `$XDG_CONFIG_HOME/msg/config.toml`, falling back to `~/.config/msg/config.toml` when the variable is unset, empty, or relative. The same convention is used on macOS. Nothing is written to `/etc`. Use `--config PATH` anywhere in the command to select another file.

```bash
msg config path
msg config list
msg config get timeout_seconds
msg config set timeout_seconds 20
msg config unset timeout_seconds
```

`config list` prints effective settings and secret references; it never resolves secret values. Writes are atomic and preserve TOML comments where possible. Concurrent config writers are not coordinated; serialize updates. A missing default file uses built-in defaults. Sending with an explicitly selected missing file is an error.

`config set` uses the setting's type: string values are literal text, and `timeout_seconds` must be an integer from 1 to 300. Use ordinary shell quoting for spaces; do not add TOML quotes around string values. Whole tables can still be set using TOML syntax for advanced edits.

See [config.example.toml](config.example.toml) for a complete editable example. To configure Slack entirely from the CLI:

```bash
msg account add work --type slack
msg config secret set work.token
msg --to 'slack:#team' 'Hello from msg'
```

Replace the channel name with your own. Account names may contain letters, digits, underscores, and hyphens. One account's token can send to many recipients passed through `--to`.

`account add` defaults to Slack and creates a keyring reference named `msg/NAME/token`; the following `config secret set` command prompts for the token. Adding an existing account fails without changing it. With one account, selection is automatic. With several accounts, use `msg config set default_slack_account work` or pass `--account NAME` when sending.

For environment or file credentials, create the reference without nested syntax:

```bash
msg account add ci --token-env MSG_SLACK_TOKEN
msg account add job --token-file /absolute/path/to/slack-token
```

Provision those credentials separately. `account add` does not read or store a token, and supports `--config PATH` for a separate configuration file.

### Updating an older configuration

Existing TOML files and table-setting commands continue to work. For individual string settings, remove any embedded TOML quotes in older scripts: use `msg config set default_slack_account work`, not `msg config set default_slack_account '"work"'`. Shell quotes such as `'work'` are fine; quotes passed through to `msg` are now literal characters.

Configured recipient aliases and defaults have been removed. Old `default_outputs`, `default_destination`, or `[destinations]` settings produce an actionable configuration error. Remove whichever entries exist:

```bash
msg config unset default_outputs
msg config unset destinations
# Only if this older singular setting exists:
msg config unset default_destination
```

These commands work even while other obsolete recipient settings remain. Accounts, secret references, and unrelated comments are preserved. For a non-default configuration file, include `--config PATH`. The application does not silently rewrite your configuration or send to old configured recipients. Recipient settings can no longer be created with `config set`.


## Slack setup

Create and install a Slack app with a bot token. Grant `chat:write` and invite the bot to the channels it should use. Private channels require bot membership. Grant `chat:write.public` only if posting to public channels without joining is required. `slack:@username` or `slack:U…` opens a DM with the bot using `conversations.open`, requiring `im:write`; an existing DM can instead be addressed as `slack:D…`.

Credentials are associated with an app installation/workspace. A bot does not inherit your personal private-channel access and cannot generally post into an existing DM between two other people. See Slack's [message API](https://docs.slack.dev/reference/methods/chat.postMessage/) and [DM API](https://docs.slack.dev/reference/methods/conversations.open/).

### Service-prefixed destinations

`slack:` selects the service, with the recipient supplied directly in `--to`. It reuses a configured account and never accepts a token in the destination string.

```bash
msg --to 'slack:#my-channel' 'Hello'
msg --to 'slack:@dustin' 'Hello'
msg --to 'slack:#my-channel' --to 'slack:@dustin' --passthrough 'Hello'

# IDs bypass name lookup and its extra read scopes.
msg --to slack:C0123456789 'Hello channel'
msg --to slack:U0123456789 'Hello user'
```

`@dustin` matches the exact Slack username (`name` in Slack's API), not a display name or full name. Deleted users and archived channels are excluded. Name lookups follow pagination, reject ambiguous matches, and finish before any messages or stdout output are sent. Each lookup is bounded by `timeout_seconds` across all its pages. Unknown/inaccessible names, missing scopes, and lookup rate limits fail preflight with exit code 2. No directory cache is persisted; using IDs avoids lookup requests.

Account selection for `slack:` destinations is:

1. `--account NAME`, if supplied.
2. `default_slack_account` in the configuration.
3. The sole configured account, if there is exactly one.

Multiple accounts without a selection cause an error. `--account` selects credentials for all Slack recipients in that invocation; it does not select a recipient.

```bash
msg config set default_slack_account work_slack
msg --account work_slack --to 'slack:@dustin' 'Hello'

```

In addition to the sending scopes above, channel name lookup uses `channels:read` for public channels and `groups:read` for private channels. Username lookup requires `users:read`; opening the resulting DM still needs `im:write`. Private channel access still requires bot membership. Grant the necessary scopes to your app and reinstall/re-authorize it in the workspace if its grants change. See Slack's [channel listing](https://docs.slack.dev/reference/methods/conversations.list/) and [user listing](https://docs.slack.dev/reference/methods/users.list/) documentation.

Only the fixed HTTPS Slack API endpoint is used in production. Requests have bounded timeouts, redirects and automatic retries are disabled, and HTTP 200 responses are checked for Slack's `ok` field. The conservative text limit is 40,000 characters after Slack escaping; use `-n` to shorten the remote message. This can be smaller than 40,000 input characters when the message contains `&`, `<`, or `>`.

## Credentials

Choose one explicit source per account. There is no fallback to another source.

| Source | Configuration | Intended use |
| --- | --- | --- |
| Keyring | `{ source = "keyring", name = "msg/work-slack/token" }` | Interactive desktop users |
| File | `{ source = "file", path = "/absolute/path/token" }` | Cron, services, injected secrets |
| Environment | `{ source = "env", name = "MSG_SLACK_TOKEN" }` | CI systems providing environment secrets |

Keyring entries use Linux Secret Service (attributes `application=msg`, `name=...`) or macOS Keychain (service `msg`, account equal to `name`). Set/replace an entry with `msg config secret set work_slack.token`; delete it with `msg config secret delete work_slack.token`. Setting requires a terminal and reads the token with echo disabled. On Linux, create a default collection in your desktop credential manager first. Setup may request OS authorization; sending never requests an unlock prompt. A locked, inaccessible, or missing entry causes an error. Linux keyring calls use a five-second D-Bus method timeout.

File credentials must be regular files with absolute paths, owned by the running account or root, with no group/other permissions (typically `0600` or `0400`). Symlinks are rejected. If a platform mounts secrets via symlinks, reference the protected regular file or provision a private regular file instead. Keep containing directories private, typically `0700`. A final CR/LF is stripped from file credentials. Files are access-controlled plaintext, not encrypted storage. File/env provisioning and deletion happen outside `msg`; `config secret` only manages keyring entries.

Use a service account's own configuration for unattended jobs. A desktop keyring may be unavailable without a login session. systemd services/timers can use `LoadCredential=` or `LoadCredentialEncrypted=` and point the file reference at the supplied credential path. See [systemd credentials](https://www.freedesktop.org/software/systemd/man/258/systemd.exec.html). Environment secrets are supported for compatibility but can leak through debugging or inherited process environments.

Tokens never appear in ordinary config output, request diagnostics, or result files. Configuration parse errors omit source lines to avoid exposing accidentally pasted secrets. Resolved token buffers are zeroed when dropped where owned by `msg`; HTTP and OS libraries may make internal copies. Avoid passing secrets in command-line arguments or placing them directly in ordinary TOML fields.

## Delivery results

All selected destinations are checked and their credentials resolved before sending begins. A preflight error produces no stdout message and sends nothing. Once delivery starts, every destination is attempted even if another fails, including a broken stdout pipe. Sends are sequential. There is no atomic transaction or rollback across destinations.

Success means Slack accepted the message or stdout was written successfully. A timeout or interrupted response can leave Slack acceptance uncertain. No automatic retries are performed; rerunning the entire invocation may duplicate messages already accepted.

| Exit code | Meaning |
| --- | --- |
| 0 | All destinations succeeded, or configuration command succeeded |
| 2 | Usage, input, configuration, credential, or preflight error; no deliveries started |
| 3 | No destination succeeded; failures may include uncertain acceptance |
| 4 | Some destinations succeeded and some failed or are uncertain |
| 5 | Deliveries were attempted but the requested result file could not be saved |

Diagnostics go to stderr. Successful sends are silent except for the stdout destination. For machine-readable per-destination results:

```bash
msg --to stdout --to 'slack:#team' --result-file results.json 'Deployment complete'
```

The JSON file contains `destination`, `status` (`written`, `accepted`, `failed`, or `uncertain`), and an optional `error`. It never contains message bodies or credentials. The parent directory must exist and the file must not already exist; choose a unique path for each run. Results are written atomically with private permissions. Preflight failures do not produce a result file.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Tests use temporary configuration/credential files and a local HTTP mock, with no real Slack sends or desktop credential-store changes. CI runs on Linux and macOS. See [notes.md](notes.md) for design context and follow-up decisions.
