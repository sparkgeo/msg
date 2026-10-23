---
title: MSG
section: 1
header: User Commands
footer: msg
lang: en
---

# Name

msg — send one text message to stdout, Slack, or several destinations

# Synopsis

```text
msg [OPTIONS] [--] [MESSAGE ...]
msg [--config PATH] account add NAME [--type slack] [--token-env NAME | --token-file PATH]
msg [--config PATH] config path
msg [--config PATH] config list
msg [--config PATH] config get KEY
msg [--config PATH] config set KEY VALUE
msg [--config PATH] config unset KEY
msg [--config PATH] config secret set ACCOUNT.token
msg [--config PATH] config secret delete ACCOUNT.token
msg help [COMMAND ...]
```

# Description

`msg` reads one UTF-8 message from arguments or standard input, delivers it
synchronously, and exits with a status suitable for shell scripts. Linux and
macOS are supported. With no `--to`, it writes to stdout; Slack delivery always
requires an explicit `--to slack:RECIPIENT`.

Configuration stores credential references and application settings, never
recipient locations. Selecting an account does not select a destination.
There are no background queues, automatic retries, streaming sends, email
destinations, or incoming-webhook destinations.

# Quick start

Prebuilt Linux and macOS packages are available from
[GitHub Releases](https://github.com/sparkgeo/msg/releases). Choose your platform
and architecture. On Debian/Ubuntu, install the matching `.deb` with
`sudo apt install ./msg_VERSION_ARCH.deb`. For a `.tar.gz` archive, extract it
and run `./install.sh` from the extracted directory to install the binary and
manual under `$HOME/.local`; keep `$HOME/.local/bin` on `PATH`. An absolute
prefix can be supplied, for example `sudo ./install.sh /usr/local`.
Release archives include installation details and example configuration.

Build and install from a source checkout with Rust 1.88 or newer, a native
compiler toolchain, and Make:

```sh
make install
msg 'Hello from the terminal'
printf 'Hello from stdin\n' | msg
man msg
```

`make install` installs the binary and the prebuilt manual under `PREFIX`,
which defaults to `$CARGO_HOME` when set, otherwise `$HOME/.cargo`. Keep
`PREFIX/bin` on `PATH`. No documentation generator is needed to install.
For another prefix, use `make install PREFIX="$HOME/.local"`.

`cargo install --path . --locked` installs only the binary. Add the manual
separately with `make install-man`, using the same prefix as the binary.
`make install-man MANDIR=/usr/local/share/man` selects a system manual
directory when you have permission to write there.

Read the manual without installing with `man ./man/msg.1` from the checkout.
In the usual `man` pager, `/timeout` searches for text, `n` finds the next
match, and `q` quits. If `man` is unavailable, install your operating system's
manual viewer (for example, `man-db` on Ubuntu).

For Slack, first create and install a Slack app with the scopes described in
**Slack setup**, then configure an account and store its token:

```sh
msg account add work --type slack
msg config secret set work.token
msg --to 'slack:#team' 'Deployment complete'
```

Replace example channel names, usernames, and IDs with your own. The secret
command prompts in the terminal with echo disabled. One configured account
is selected automatically for all Slack destinations.

# Options

**--config PATH**
: Use this TOML file instead of the default configuration path. Relative paths
  are resolved from the working directory. This global option can appear
  before or after a `config` or `account` subcommand. An explicitly selected file must
  exist when sending. Configuration commands can inspect defaults or create
  a file even when this path does not yet exist.

**-t DESTINATION, --to DESTINATION**
: Select a destination. Repeat to select several. Supported forms are
  `stdout`, `slack:#channel`, `slack:@username`, and `slack:ID`.
  Default when omitted: `stdout`. Once any `--to` is present, stdout is
  included only if explicitly selected or added with `--passthrough`.
  See **Destinations** for syntax, ordering, and duplicate handling.

**--account NAME**
: Select the configured Slack account for every Slack destination in this
  invocation. Overrides `default_slack_account`. The name must exist even
  if only stdout is selected. This option never changes the recipient.

**-p, --passthrough**
: Add stdout as the first destination. Stdout receives the entire original
  message, even when `--max-chars` shortens the Slack message. If stdout
  was already selected, it is still written only once. All preflight checks
  finish before any output, including passthrough output.

**--no-newline**
: Suppress the extra newline normally appended to argument messages on
  stdout. Existing newlines are preserved. Does not change piped input or
  Slack text. The default is to append a newline for argument messages.

**-n N, --max-chars N**
: Send only the first N Unicode scalar values to Slack. N must be a positive
  integer; zero and negative values are errors. Default: no truncation.
  Stdout stays complete. Counting is by scalar value, not UTF-8 byte or
  displayed grapheme: an emoji sequence or a combining character may be
  split. No ellipsis is added. Truncation occurs before Slack escaping;
  the escaped message must still fit the 40,000-character limit.
  This option does **not** suppress the stdout newline.

**--result-file PATH**
: Save a JSON array of per-destination results after delivery. The parent
  directory must already exist and the path must not exist. Relative paths
  are relative to the working directory. `-` is rejected; results never
  replace stdout. The report contains neither message text nor token values.
  See **Delivery and results** for the schema and failure behavior.

**-h, --help**
: Print command help and exit successfully. Available on subcommands too;
  for example, `msg config set --help`. `--help` provides the longer help
  form where Clap has additional detail. The generated `help` subcommand
  also accepts a command path, such as `msg help config secret set`.

**-V, --version**
: Print the application version and exit successfully.

**--**
: End option and subcommand parsing. Use before message text that begins
  with a hyphen or would otherwise be recognized as a subcommand, such as
  `msg -- config list`.

**MESSAGE ...**
: Message words, joined with one space between arguments. Omit all message
  arguments to read stdin through EOF. Quote text to preserve spaces inside
  an argument. An empty message is rejected.

Send options cannot be combined with `config` subcommands. Only `--config`
is global. Put send options before the message for clarity.

# Input and output

Argument messages behave like basic `echo`: the shell supplies the arguments,
`msg` joins them with spaces, and stdout appends a newline. If the argument
already ends with a newline, the added newline is still appended unless
`--no-newline` is set. Literal backslashes are not interpreted as escape
sequences. Slack receives the joined message without an added newline.

```sh
msg Hello world                  # stdout: Hello world plus newline
msg 'Hello   world'              # preserve the three spaces
msg --no-newline 'Working...'    # no added newline
msg 'first\nsecond'              # literal backslash and n
msg -- '-a message beginning with a flag'
msg -- config list              # send the words "config list"
printf 'first\nsecond\n' | msg   # two lines, preserved exactly
```

Piped or redirected stdin is preserved exactly on stdout, including leading
whitespace, CRLF, and trailing newlines. Input is buffered through EOF before
delivery. Both argument and stdin messages are limited to 8 MiB (8,388,608
UTF-8 bytes); `-n` does not increase that limit. Invalid UTF-8 and empty input
are errors. Whitespace-only input is allowed locally, though Slack may reject
it. Running `msg` without arguments on an interactive terminal is an error;
it does not start an interactive message editor.

An argument plus nonempty stdin is rejected to avoid silently discarding
input. Empty redirected stdin is allowed with an argument:

```sh
msg 'Service started' </dev/null
msg < report.txt
```

An ongoing stream such as `tail -f` does not produce incremental deliveries;
`msg` waits for EOF (or the input limit). Redirecting stdout does not add a
stdout destination when Slack was explicitly selected.

Slack messages are sent as plain text. `&`, `<`, and `>` are escaped;
markup, automatic mention/link parsing, and link/media previews are disabled.
Stdout is never Slack-escaped. The local Slack size check is 40,000 Unicode
scalar values **after escaping**, so text containing many ampersands or angle
brackets may need a smaller `-n` value.

# Destinations

**stdout**
: Write the original message to standard output. No account or token is
  needed. Configuration must still be valid.

**slack:#channel**
: Look up an exact Slack channel name, excluding archived channels. Lookup
  requests include public and private channels and need the corresponding
  read scopes. Private channels must be accessible to the bot.

**slack:@username**
: Look up the exact Slack API `name`, excluding deleted users. Display names
  and full names are not matched. The bot opens a DM with that user before
  posting; this is not a message in your personal DM conversation.

**slack:C…, slack:G…, slack:D…**
: Send to a channel/conversation ID directly, including an existing DM ID.
  Bypasses directory lookup. Permissions and bot access still apply.

**slack:U…, slack:W…**
: Open a bot DM with this user ID and then send the message. Bypasses username
  lookup but still requires permission to open the DM.

Names after `#` or `@` must be nonempty, at most 255 UTF-8 bytes, and contain
only Unicode letters/digits, hyphens, underscores, or periods. IDs must
start with one of the uppercase prefixes above, contain at least two
characters, and consist entirely of uppercase ASCII letters and digits.
These are local syntax checks; Slack determines whether the recipient exists.
Destination strings are case-sensitive. Unprefixed aliases and other service
prefixes are unsupported. Tokens are never accepted inside a destination.

Repeated identical destination strings are delivered once, in first-occurrence
order. `--passthrough` puts stdout first. Different strings that resolve to
the same recipient, such as a name and its ID, are separate deliveries.

All directory lookups finish before any message is sent or stdout written.
Lookup follows pagination and requires exactly one matching ID. Unknown or
inaccessible names, ambiguous matches, missing scopes, malformed responses,
and lookup rate limits fail preflight with exit code 2. Each complete lookup
has a `timeout_seconds` budget across its pages and a 1,000-page safety limit.
No directory cache is persisted between invocations. IDs avoid these lookup
requests and their additional read scopes.

# Configuration

## File selection and defaults

Configuration is TOML. The file is selected in this order:

1. `--config PATH`, when supplied.
2. `$XDG_CONFIG_HOME/msg/config.toml` when `XDG_CONFIG_HOME` is nonempty and
   absolute.
3. `$HOME/.config/msg/config.toml` otherwise, including when `XDG_CONFIG_HOME`
   is empty or relative.

Linux and macOS use the same convention. Nothing is read from or written to
`/etc` automatically. If neither a usable XDG path nor HOME is available,
use `--config`. Paths inside TOML do not expand `~` or `$VARIABLE`; use a
literal absolute path for file credentials.

A missing default file means built-in defaults. Sending with an explicit
missing file is an error. An existing invalid file is an error even for
stdout-only sends. Unknown fields, wrong types, and invalid values are
rejected. There are no environment overrides for the settings below.

The complete default configuration is:

```toml
timeout_seconds = 30
[accounts]
```

`default_slack_account` is absent by default. There are no implicit accounts,
secrets, or remote recipients.

## timeout_seconds

Integer from **1 through 300**, inclusive. Default: **30**.
Controls the total timeout of each Slack HTTP request. Connection setup is
also bounded by the smaller of this value and 10 seconds. A paginated name
lookup shares one timeout budget across all its pages. Opening a DM and
posting its message are separate requests. Multiple destinations are sent
sequentially, so this is not a deadline for the whole command. It does not
limit waiting for stdin or replace the keyring's own timeouts.

```sh
msg config set timeout_seconds 60
msg config get timeout_seconds
msg config unset timeout_seconds   # return to 30
```

## default_slack_account

Optional string naming the account to use when `--account` is absent.
There is no default value. Names use ASCII letters, digits, underscores, and
hyphens and must not be empty. The selected account must exist when used for
Slack delivery; a syntactically valid reference can be saved before creating
the account.

Credential selection is: `--account NAME`, then `default_slack_account`, then
the sole configured account. If no account exists, or several exist without
a selection, Slack delivery fails before sending. A bad explicit/default
selection does not fall back to another account.

```sh
msg config set default_slack_account work
msg --account personal --to 'slack:#general' 'Hello'
msg config unset default_slack_account
```

## accounts and accounts.NAME.type

`accounts` is a table of named accounts, empty by default. Account names
follow the same rules as `default_slack_account`. Each account requires
`type` and `token_secret`. The only supported `type` is the string `"slack"`.
An account represents a token for one Slack app installation/workspace;
the same account can send to many recipients in that workspace.

```toml
[accounts.work]
type = "slack"

[accounts.work.token_secret]
source = "keyring"
name = "msg/work/token"
```

## accounts.NAME.token_secret

Required table describing **where** to retrieve the token, never the token
itself. Exactly one of these three shapes is allowed:

```toml
# Desktop credential store:
token_secret = { source = "keyring", name = "msg/work/token" }

# A secret supplied by a CI runner or parent process:
token_secret = { source = "env", name = "MSG_SLACK_TOKEN" }

# A protected file for a service or scheduled job:
token_secret = { source = "file", path = "/absolute/path/slack-token" }
```

These are alternatives: put only one `token_secret` in each account.

**source**
: Required string, exactly `"keyring"`, `"env"`, or `"file"`. There is no
  default and no fallback if the selected source is unavailable.

**name**
: Required for `keyring` and `env`; not allowed for `file`. Must be nonempty
  and contain no control characters. For `keyring`, this is the entry name.
  For `env`, it is the exact, case-sensitive environment variable name, not
  its value; ordinary shell-compatible names such as `MSG_SLACK_TOKEN` are
  easiest to provision.

**path**
: Required for `file`; not allowed for `keyring` or `env`. Must be an absolute
  path to a protected regular file. The file is read when Slack credentials
  are needed, not by `config list` or `config get`.

Changing source usually requires replacing the whole `token_secret` table
so old fields do not conflict with the new shape:

```sh
msg config set accounts.work.token_secret \
  '{ source = "env", name = "MSG_SLACK_TOKEN" }'
```

## Complete multi-account example

```toml
timeout_seconds = 45
default_slack_account = "work"

[accounts.work]
type = "slack"
token_secret = { source = "keyring", name = "msg/work/token" }

[accounts.personal]
type = "slack"
token_secret = { source = "env", name = "PERSONAL_SLACK_TOKEN" }

[accounts.automation]
type = "slack"
token_secret = { source = "file", path = "/run/credentials/notify/slack-token" }
```

The example credential file must be provisioned before using `automation`.
Setting a default account still leaves `msg 'Hello'` as stdout-only. To send
with credentials from different accounts, use separate invocations:

```sh
msg --account work --to 'slack:#team' 'Work update'
msg --account personal --to 'slack:#general' 'Personal update'
```

# Configuration commands

**msg account add NAME [--type slack] [--token-env NAME | --token-file PATH]**
: Create a complete account in one write. The type defaults to `slack`,
  currently the only supported service. Without a credential option, create
  a keyring reference named `msg/NAME/token`; store the token afterward with
  `msg config secret set NAME.token`. `--token-env` references an environment
  variable, and `--token-file` references an absolute file path. These options
  are mutually exclusive. Credentials are provisioned separately and are
  not read or checked by this command. Existing accounts are never overwritten.
  A missing configuration file and its parent directories are created.
  The command does not set a default account; a sole account is selected
  automatically, and multiple accounts need a default or `--account`.

  Examples:

  ```sh
  msg account add work
  msg config secret set work.token
  msg account add ci --token-env MSG_SLACK_TOKEN
  msg account add job --token-file /absolute/path/to/slack-token
  msg config set default_slack_account work
  ```

**msg config path**
: Print the absolute selected configuration path. Does not read, validate,
  or create the file.

**msg config list**
: Print the effective configuration as TOML, including defaults and secret
  references. Does not resolve tokens or prove the credentials work.
  A missing file prints defaults without creating it.

**msg config get KEY**
: Print a value from the effective configuration using a dotted key, such as
  `accounts.work.type`. Strings are printed without surrounding TOML quotes;
  other values use TOML representation. Missing keys are errors. For example,
  an unset `timeout_seconds` reads as `30`, while an unset
  `default_slack_account` has no value and is an error.

**msg config set KEY VALUE**
: Set a dotted key using the setting's type. String settings take literal
  text, including values such as `123`, `true`, or `[entry]`. Use shell quotes
  for spaces, but do not embed TOML quotes; any quotes passed through the
  shell become part of the stored string. `timeout_seconds` takes an integer
  from 1 through 300. Whole tables still accept TOML syntax, including quoted
  strings inside those tables, for advanced edits.
  Each dotted key component must follow the account-name character rules.
  The entire resulting configuration is validated before writing. Use
  `account add` to create accounts, because partial accounts are invalid.
  Missing parent directories and a missing configuration file are created.

  Existing TOML files need no migration. Older scripts that embedded TOML
  quotes around individual string values should remove them: use
  `msg config set default_slack_account work` instead of
  `msg config set default_slack_account '"work"'`.

**msg config unset KEY**
: Remove a stored setting. Built-in defaults apply again where defined.
  Unlike `get`, this operates on the file, so removing a key that was never
  stored is an error. The remaining configuration must be valid; remove a
  whole account instead of removing one of its required fields. Removing an
  account does not delete its keyring entry, file, or environment variable.

**msg config secret set ACCOUNT.token**
: Create or replace the keyring entry referenced by this configured account.
  Requires a terminal for a hidden token prompt. The account must already
  exist and use `source = "keyring"`. OS authorization/unlock prompts may
  occur during setup. A `.token` suffix is required; this is an account
  reference, not the keyring entry's name. No token value is accepted as
  an argument or via piped stdin.

**msg config secret delete ACCOUNT.token**
: Delete that account's configured keyring entry. Keeps the account and
  secret reference in TOML; future Slack sends fail until the entry is
  restored or the reference changes. File and environment secrets must be
  managed outside `msg`. Accounts sharing one keyring name share the entry,
  so replacing or deleting it affects all of them.

```sh
msg config path
msg config list
msg config get accounts.work.token_secret.source
msg config set accounts.work.token_secret.name msg/work/new-token
msg config secret set work.token

# Use an isolated configuration; account add creates it if needed.
msg --config ./job.toml account add job --token-env JOB_TOKEN
msg config list --config ./job.toml

# Delete a keyring entry before removing its account reference.
msg config secret delete work.token
msg config unset accounts.work
# Remove a default referring to work too, if it was stored:
msg config unset default_slack_account
```

Writes replace the configuration atomically with private file permissions
and preserve comments where possible. Newly created directories use mode
0700; existing directory permissions are not changed. Concurrent writers
are not coordinated: serialize configuration updates to avoid lost changes.
Rejected edits leave the previous file intact. Source lines are omitted
from parse errors to avoid exposing accidentally pasted secrets.

## Migrating older configurations

`default_outputs`, `default_destination`, and `[destinations]` are obsolete
and rejected. Named destination aliases are also unsupported. Remove only
the entries present in your file, then pass recipients through `--to`:

```sh
msg config unset default_outputs
msg config unset destinations
msg config unset default_destination
msg --to 'slack:#team' 'Using an explicit recipient'
```

These removals can be performed one at a time while other obsolete
recipient settings remain. Accounts and unrelated comments are preserved.
For a different file, add `--config PATH`. No automatic migration sends to
old recipients or rewrites your configuration.

# Credentials

All resolved tokens must be nonempty, at most 16 KiB (16,384 bytes), and
contain only printable ASCII without spaces. Environment and keyring tokens
are not whitespace-trimmed. File contents must fit the 16 KiB limit before
trailing CR/LF characters are stripped. Embedded newlines and other whitespace
are invalid. Tokens are resolved only for accounts needed by selected Slack
destinations.

## Keyring

Linux uses Secret Service on the session D-Bus, with attributes
`application=msg` and `name=NAME`. Create a default collection in your
desktop credential manager before storing a token. Linux D-Bus method calls
have a five-second timeout. macOS uses Keychain service `msg` with account
`NAME`. Sending never requests an unlock/authorization prompt: missing,
locked, inaccessible, or ambiguous entries fail instead. Unlock/authorize
the store interactively before sending, or choose a file/env source for
unattended jobs.

```sh
msg config secret set work.token       # store or rotate
msg config secret delete work.token    # revoke local stored copy
```

Deleting a local copy does not revoke the token at Slack.

## File

The credential path must be a regular file, readable by the process, owned
by the current effective user or root, and have no group/other permission
bits. Typical modes are 0600 or 0400. A symlink at the credential path is
rejected; symlinks in parent directories are not checked by this rule.
Keep containing directories private, typically 0700. The token is stored
as access-controlled plaintext, not encrypted by `msg`.

In Bash, provision a new private file without putting the token on a command
line or in shell history:

```bash
mkdir -p "$HOME/.config/msg/secrets"
chmod 700 "$HOME/.config/msg/secrets"
(
  umask 077
  read -r -s -p 'Slack token: ' token
  printf '\n' >&2
  printf '%s\n' "$token" > "$HOME/.config/msg/secrets/slack-token"
)
chmod 600 "$HOME/.config/msg/secrets/slack-token"
```

Then set the reference, replacing `/home/alice` with your actual home path:

```sh
msg config set accounts.work.token_secret \
  '{ source = "file", path = "/home/alice/.config/msg/secrets/slack-token" }'
```

For mounted secrets, reference a protected regular file. A symlink supplied
by an orchestrator is not accepted directly. Provision and remove these
files outside `msg`.

## Environment

The named variable must be exported into `msg`'s environment. A CI secret
can be mapped to `MSG_SLACK_TOKEN`, with this account configuration:

```toml
[accounts.ci]
type = "slack"
token_secret = { source = "env", name = "MSG_SLACK_TOKEN" }
```

For a temporary interactive Bash session:

```bash
read -r -s -p 'Slack token: ' MSG_SLACK_TOKEN
printf '\n' >&2
export MSG_SLACK_TOKEN
msg --account ci --to slack:C0123456789 'CI completed'
unset MSG_SLACK_TOKEN
```

Environment secrets can be exposed by debug output or inherited by child
processes. Avoid shell tracing while provisioning credentials. `config
secret` commands do not set or unset environment variables.

Ordinary configuration output, result files, and request diagnostics omit
resolved token values. Owned token buffers are zeroed when dropped where
controlled by `msg`; HTTP and OS libraries may make internal copies. Do not
put tokens in message text, destination strings, or ordinary TOML fields.

# Slack setup

Create a Slack app, grant the appropriate **bot token** scopes, and install
it into the workspace. Store the installation's token in your chosen secret
source. Invite the bot to the channels where it will post.

**chat:write**
: Send messages. Required for all Slack posting.

**chat:write.public**
: Post to public channels without joining, in addition to `chat:write`.

**channels:read**
: Look up public channel names.

**groups:read**
: Look up private channel names.

**users:read**
: Look up usernames.

**im:write**
: Open a DM from a username or user ID.

Private channels require bot membership. A bot does not inherit your
personal access to private conversations and cannot generally post into an
existing DM between other people. Supplying a channel or existing DM ID
avoids directory lookup; supplying a user ID still requires opening a DM.
If you change scopes, reinstall/re-authorize the app to update its grants.

The production API endpoint is fixed to `https://slack.com/api`. HTTP
redirects and automatic retries are disabled. HTTP 200 alone does not mean
success: `msg` checks Slack's `ok` field. The HTTP client uses standard
proxy environment settings; there is no msg-specific proxy or endpoint
configuration key.

API details: [chat.postMessage](https://docs.slack.dev/reference/methods/chat.postMessage/),
[conversations.open](https://docs.slack.dev/reference/methods/conversations.open/),
[conversations.list](https://docs.slack.dev/reference/methods/conversations.list/),
and [users.list](https://docs.slack.dev/reference/methods/users.list/).

# Delivery and results

Before any delivery, `msg` validates the configuration, input, destination
syntax, selected credentials, escaped Slack length, result-file destination,
and directory lookups. A preflight failure prints diagnostics to stderr,
returns 2, writes no stdout message, sends no message, and creates no result
file. Preflight may read credentials and make directory lookup requests;
it is not an offline dry run.

After preflight, delivery is sequential in destination order. Every
destination is attempted even if another fails, including when stdout has
a broken pipe. Opening a user DM happens during this delivery phase; a DM
open failure is a destination failure and other deliveries still proceed.
There is no transaction or rollback across destinations.

Success means stdout was written/flushed, or Slack accepted the API request.
It does not mean a human read the message. Timeouts or interrupted/invalid
responses can leave acceptance uncertain. No automatic retries occur.
Rerunning a command can duplicate messages already accepted.

Successful sends are silent except for selected stdout output. Diagnostics
go to stderr. With `--result-file`, an example successful report is:

```json
[
  { "destination": "stdout", "status": "written" },
  { "destination": "slack:C0123456789", "status": "accepted" }
]
```

Each array entry has the original destination string and one status:

**written**
: Stdout was written and flushed successfully.

**accepted**
: Slack acknowledged the message.

**failed**
: The destination failed; an `error` string explains why. Stdout failure may
  leave partial output. A rejected Slack request, rate limit, connection
  failure, or failure to open a DM is reported here.

**uncertain**
: Slack acceptance could not be determined; an `error` string explains why.
  Examples include request interruption, server errors, or malformed posting
  responses. Check the destination before deciding to resend.

The report is written atomically with private permissions and refuses to
overwrite an existing file, including a file created at the same path during
delivery. The parent directory must already exist. If saving fails after
deliveries were attempted, exit code 5 overrides the delivery outcome.
There is no report for a preflight failure. Reports omit message bodies and
credentials but do include recipient labels.

# Exit status

**0**
: All destinations succeeded, or a configuration/help/version command succeeded.

**2**
: Usage, input, configuration, credential, or preflight error; no deliveries started.

**3**
: No destination succeeded; acceptance may be uncertain.

**4**
: Some destinations succeeded and some failed or are uncertain.

**5**
: Deliveries were attempted, but the requested result file could not be saved.

# Examples

## Several destinations and full stdout output

```sh
msg -t 'slack:#team' -t 'slack:#operations' 'Deployment complete'
msg -p -t 'slack:#team' -t 'slack:@alice' 'Please review'
msg -t stdout -t slack:C0123456789 'Saved locally and sent remotely'
```

## Short Slack notification, complete local report

```sh
# report.txt already contains the full report.
msg --to 'slack:#team' --max-chars 500 --passthrough \
  < report.txt > report-copy.txt
```

Only Slack is truncated. The copied file contains all the original bytes.

## Check a pipeline's status

In Bash, `pipefail` makes failures earlier in a pipeline affect its exit
status. Without it, the shell usually reports only the last command's status.
`msg` cannot know whether the upstream producer succeeded; it may send the
partial output of a failed command.

```bash
set -o pipefail
printf 'Build complete\n' | msg --to 'slack:#team' --passthrough | tee notice.txt
```

To send only if a producer succeeds, capture its output first. Replace
`./build-report` with your actual command:

```sh
report_dir=$(mktemp -d)
trap 'rm -rf "$report_dir"' EXIT
if ./build-report > "$report_dir/report.txt"; then
  msg --to 'slack:#team' < "$report_dir/report.txt"
else
  printf '%s\n' 'Report failed; nothing sent' >&2
fi
```

## Capture results and distinguish failure types

Use a new path inside an existing private directory. Do not pass a file
created by `mktemp` itself: `--result-file` refuses existing files.

```sh
result_dir=$(mktemp -d)
status=0
msg --to stdout --to 'slack:#team' \
  --result-file "$result_dir/results.json" 'Deployment complete' || status=$?
case "$status" in
  0) printf '%s\n' 'All deliveries succeeded' ;;
  2) printf '%s\n' 'Preflight failed; no messages sent' >&2 ;;
  3|4) printf '%s\n' 'Inspect results before retrying' >&2 ;;
  5) printf '%s\n' 'Report missing; deliveries were attempted' >&2 ;;
esac
if [ -f "$result_dir/results.json" ]; then
  cat "$result_dir/results.json"
fi
```

Keep or remove the result directory after reviewing it. With `jq` installed,
`jq '.[] | select(.status == "uncertain")' PATH` selects uncertain results.

## Cron and service accounts

Use absolute binary/configuration paths, an explicit `--to`, and a file or
environment secret provisioned for the service user. A desktop keyring may
be unavailable outside a login session. For example, a crontab entry with
paths adapted to your installation:

```text
0 9 * * * /home/alice/.cargo/bin/msg --config /home/alice/.config/msg/job.toml --to slack:C0123456789 'Daily job started'
```

For systemd, `LoadCredential=` or `LoadCredentialEncrypted=` can provision a
credential file. Set the account's `token_secret.path` to its absolute
runtime path. TOML does not expand systemd specifiers or environment
variables. The supplied file must satisfy **Credentials — File**.

## Local-only checks without Slack

```sh
msg --version
msg --help
msg config set --help
msg config path
msg config list
printf 'Local test\n' | msg --to stdout
```

These commands do not send Slack messages. Configuration inspection does
not resolve secrets or verify remote access. There is no `--dry-run` flag.

# Environment

**XDG_CONFIG_HOME**
: Absolute base directory for the default config path. Empty/relative values
  fall back to HOME. It does not choose the manual installation directory.

**HOME**
: Used to derive `~/.config/msg/config.toml` when XDG_CONFIG_HOME is unusable.
  Not required for path selection when an explicit config or usable XDG base
  is supplied.

**Variables named by token_secret.name when source is env**
: Provide account credentials. There is no fixed built-in token variable;
  `MSG_SLACK_TOKEN` is an example name only.

**HTTPS_PROXY, HTTP_PROXY, ALL_PROXY, NO_PROXY**
: Standard proxy environment variables recognized by the underlying HTTP
  client (lowercase forms are also supported). The Slack endpoint is HTTPS,
  so an HTTPS/all-protocol proxy is relevant. These are not TOML settings.

**MANPATH, MANPAGER, PAGER**
: Used by the system `man` viewer, not by `msg`. If the installed manual is
  not discovered, add its parent man directory to MANPATH with a trailing
  colon to retain the viewer's default search path:

```sh
export MANPATH="$HOME/.cargo/share/man:${MANPATH:-}"
man msg
```

Adjust the path for your installation prefix. Add it to your shell startup
file if needed. `man -M "$HOME/.cargo/share/man" msg` selects the directory
for one invocation.

# Files

**$XDG_CONFIG_HOME/msg/config.toml or $HOME/.config/msg/config.toml**
: Default settings and credential references.

**Files referenced by accounts.NAME.token_secret.path**
: Plaintext token files with restricted ownership and permissions.

**PATH supplied to --result-file**
: Optional private JSON delivery report. Must be a new file.

**PREFIX/share/man/man1/msg.1**
: Installed section 1 manual, unless `MANDIR` was overridden at installation.

# Troubleshooting

**No manual entry for msg**
: Run `make install-man` from the checkout. Ensure the corresponding
  `PREFIX/bin` is on PATH so the viewer can infer `PREFIX/share/man`, or set
  MANPATH as above. With a custom prefix, pass the same `PREFIX` or `MANDIR`
  when installing the manual. `man ./man/msg.1` always allows local viewing.

**Configuration or schema error**
: Check `msg config path`, the TOML syntax, and the exact keys/types in
  **Configuration**. Inline token values and unknown keys are rejected.
  Source text is intentionally omitted from diagnostics. Remove obsolete
  recipient settings with `config unset` if present.

**Multiple accounts or unknown account**
: Supply `--account NAME` or set `default_slack_account` to an existing
  account. `msg config list` shows the configured names.

**Keyring unavailable, locked, or missing**
: Unlock/authorize the credential store interactively, check the configured
  entry name, and store the token using `config secret set ACCOUNT.token`.
  On Linux, ensure a session bus and default collection exist. For services,
  provision a file/env source instead.

**Credential file rejected**
: Use an absolute path to a readable regular file with mode 0600 or 0400,
  owned by the process user or root. Do not point directly at a symlink.
  Check token size and whitespace without printing the token to logs.

**Slack name not found, ambiguous, or lookup timed out**
: Check the exact channel/API username, workspace, read scopes, and bot
  membership. Try a Slack ID to bypass directory lookup. Large directories
  may require a larger timeout; the maximum is 300 seconds.

**Slack authentication or scope error**
: Check the selected account and rotate/re-provision a revoked token. Grant
  the required scopes and reinstall/re-authorize the app. Config inspection
  alone does not test authentication.

**Slack destination unavailable or DM cannot open**
: Check recipient IDs, bot membership, and `im:write` for user destinations.
  Private conversations are limited by the bot's access.

**Rate limit or uncertain acceptance**
: `msg` does not retry. A rate-limit diagnostic may include a retry delay.
  Lookup rate limits stop preflight; posting failures appear in delivery
  results. Inspect successful/uncertain destinations before resending.

**Message too long**
: Use `-n N` with a smaller positive value for Slack. Escaping can increase
  length. The 8 MiB input limit applies before truncation; supply smaller
  input if that limit is exceeded.

**No stdout while sending to Slack**
: Add `--passthrough` or `--to stdout`. Redirection alone does not select it.
  Preflight failures suppress stdout even when passthrough was requested.

**Command waits indefinitely for piped input**
: Close the producer or use a finite input file. Input is buffered to EOF;
  `timeout_seconds` only bounds Slack requests, not stdin reading.

**Result file exists or cannot be saved**
: Choose a new filename in an existing writable directory. Preflight
  failures return 2. Exit 5 means deliveries were attempted before saving
  failed, so check recipients before retrying.

# See also

`man(1)`, `printf(1)`, `chmod(1)`, `crontab(5)`, `systemd.exec(5)`.

Use `msg --help`, `msg account add --help`, `msg config --help`, and `msg config secret --help` for
compact command summaries. The web and terminal manuals are generated from
the same `docs/manual.md` source in the project repository.
