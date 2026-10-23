use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "Send one message to stdout and Slack")]
pub struct Cli {
    /// Configuration file (default: $XDG_CONFIG_HOME/msg/config.toml)
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Option<Command>,
    #[command(flatten)]
    pub send: SendArgs,
}

#[derive(Args)]
pub struct SendArgs {
    /// stdout, slack:#channel, slack:@username, or slack:ID; repeat for several (default: stdout)
    #[arg(short = 't', long = "to")]
    pub destinations: Vec<String>,
    /// Slack account to use for all Slack recipients
    #[arg(long)]
    pub account: Option<String>,
    /// Also write the entire original message to stdout, even with -n
    #[arg(short = 'p', long)]
    pub passthrough: bool,
    /// Do not append a newline to argument messages on stdout; preserve input newlines
    #[arg(long)]
    pub no_newline: bool,
    /// Limit Slack message length to N Unicode characters; stdout stays unchanged
    #[arg(short = 'n', long, value_parser = clap::value_parser!(usize))]
    pub max_chars: Option<usize>,
    /// Write per-destination results as JSON to a file (never stdout)
    #[arg(long)]
    pub result_file: Option<PathBuf>,
    /// Message words joined by spaces; stdout adds a newline unless --no-newline. Omit for stdin
    #[arg(value_name = "MESSAGE", num_args = 0..)]
    pub message: Vec<String>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Create accounts with a credential reference
    Account {
        #[command(subcommand)]
        command: AccountCommand,
    },
    /// Inspect and change settings or manage an account's stored token
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Subcommand)]
pub enum AccountCommand {
    /// Add an account (default credential: keyring entry msg/NAME/token)
    Add {
        /// Account name: letters, digits, underscores, and hyphens
        name: String,
        /// Messaging service
        #[arg(long = "type", value_enum, default_value = "slack")]
        kind: crate::config::AccountType,
        /// Read the token from this environment variable instead of the keyring
        #[arg(long, value_name = "NAME", conflicts_with = "token_file")]
        token_env: Option<String>,
        /// Read the token from this absolute file path instead of the keyring
        #[arg(long, value_name = "PATH")]
        token_file: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Print the configuration file path
    Path,
    /// Print effective configuration; secret references only
    List,
    /// Print a setting, using a dotted key
    Get { key: String },
    /// Set a dotted key; strings are literal, numbers are validated, tables use TOML
    Set { key: String, value: String },
    /// Remove a setting (built-in defaults apply again)
    Unset { key: String },
    /// Store or delete an account's token in its configured keyring
    Secret {
        #[command(subcommand)]
        command: SecretCommand,
    },
}

#[derive(Subcommand)]
pub enum SecretCommand {
    /// Prompt with echo disabled; reference is ACCOUNT.token
    Set { reference: String },
    /// Delete the configured keyring entry; keep the reference in config
    Delete { reference: String },
}
