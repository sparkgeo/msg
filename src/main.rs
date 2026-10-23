mod cli;
mod config;
mod delivery;
mod secrets;
mod slack;
mod target;

use anyhow::{Context, Result, bail, ensure};
use clap::Parser;
use cli::{AccountCommand, Cli, Command, ConfigCommand, SecretCommand};
use std::{
    io::{self, IsTerminal, Write},
    path::Path,
    process::ExitCode,
};
use zeroize::Zeroizing;

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("msg: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<u8> {
    let required = cli.config.is_some();
    let path = config::config_path(cli.config)?;
    if let Some(command) = cli.command {
        ensure!(
            cli.send.destinations.is_empty()
                && !cli.send.passthrough
                && !cli.send.no_newline
                && cli.send.max_chars.is_none()
                && cli.send.result_file.is_none()
                && cli.send.message.is_empty()
                && cli.send.account.is_none(),
            "send options cannot be combined with config or account subcommands"
        );
        return match command {
            Command::Config { command } => manage_config(&path, command),
            Command::Account { command } => manage_account(&path, command),
        };
    }
    let doc = config::read_document(&path, required)?;
    let config = config::decode(&doc)?;
    let stdin = io::stdin();
    let argument = (!cli.send.message.is_empty()).then(|| cli.send.message.join(" "));
    let message = delivery::read_input(argument, stdin.lock(), stdin.is_terminal())?;
    let plan = delivery::Plan::prepare(&config, &cli.send, message)?;
    let slack = if plan.needs_slack() {
        Some(slack::Slack::new(config.timeout_seconds)?)
    } else {
        None
    };
    // Prepare result output before any deliveries. Refuse overwrites so a report cannot
    // accidentally replace configuration, credentials, or the redirected input file.
    let report = cli
        .send
        .result_file
        .as_ref()
        .map(|path| -> Result<_> {
            ensure!(
                path != Path::new("-"),
                "--result-file must name a file, not stdout"
            );
            ensure!(
                !path.exists(),
                "result file already exists; choose a new path"
            );
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            tempfile::NamedTempFile::new_in(parent)
                .context("cannot prepare result file; parent directory must exist")
        })
        .transpose()?;

    let outcomes = plan.execute(slack.as_ref(), &mut io::stdout().lock())?;
    for outcome in &outcomes {
        if let Some(error) = &outcome.error {
            eprintln!("msg: {}: {error}", outcome.destination);
        }
    }
    if let (Some(mut report), Some(path)) = (report, cli.send.result_file.as_ref()) {
        let result = (|| -> Result<()> {
            serde_json::to_writer_pretty(&mut report, &outcomes)?;
            report.write_all(b"\n")?;
            report.as_file().sync_all()?;
            report
                .persist_noclobber(path)
                .map_err(|error| error.error)?;
            Ok(())
        })();
        if result.is_err() {
            eprintln!(
                "msg: deliveries attempted, but result file could not be saved; do not blindly resend"
            );
            return Ok(5);
        }
    }
    Ok(delivery::exit_code(&outcomes))
}

fn manage_account(path: &Path, command: AccountCommand) -> Result<u8> {
    let AccountCommand::Add {
        name,
        kind,
        token_env,
        token_file,
    } = command;
    let token_secret = match (token_env, token_file) {
        (Some(name), None) => config::SecretRef::Env { name },
        (None, Some(path)) => config::SecretRef::File { path },
        (None, None) => config::SecretRef::Keyring {
            name: format!("msg/{name}/token"),
        },
        (Some(_), Some(_)) => bail!("choose either --token-env or --token-file"),
    };
    let mut doc = config::read_document(path, false)?;
    config::add_account(&mut doc, &name, config::Account { kind, token_secret })?;
    config::atomic_write(path, doc.to_string().as_bytes())?;
    Ok(0)
}

fn manage_config(path: &Path, command: ConfigCommand) -> Result<u8> {
    if matches!(command, ConfigCommand::Path) {
        println!("{}", path.display());
        return Ok(0);
    }
    let mut doc = config::read_document(path, false)?;
    let command = match command {
        ConfigCommand::Set { key, value } => {
            config::modify(&mut doc, &key, Some(&value))?;
            config::atomic_write(path, doc.to_string().as_bytes())?;
            return Ok(0);
        }
        ConfigCommand::Unset { key } => {
            config::modify(&mut doc, &key, None)?;
            config::atomic_write(path, doc.to_string().as_bytes())?;
            return Ok(0);
        }
        other => other,
    };
    let config = config::decode(&doc)?;
    match command {
        ConfigCommand::Path => unreachable!(),
        ConfigCommand::List => print!("{}", toml::to_string_pretty(&config)?),
        ConfigCommand::Get { key } => println!("{}", config::get(&config, &key)?),
        ConfigCommand::Set { .. } | ConfigCommand::Unset { .. } => unreachable!(),
        ConfigCommand::Secret { command } => {
            let reference = match &command {
                SecretCommand::Set { reference } | SecretCommand::Delete { reference } => reference,
            };
            let account = reference
                .strip_suffix(".token")
                .context("use ACCOUNT.token as the secret reference")?;
            let account = config.accounts.get(account).context("unknown account")?;
            if !matches!(account.token_secret, config::SecretRef::Keyring { .. }) {
                bail!("this account uses a file/env secret; provision it outside msg");
            }
            match command {
                SecretCommand::Set { .. } => {
                    ensure!(
                        io::stdin().is_terminal(),
                        "secret set requires a terminal for a hidden prompt"
                    );
                    let token = Zeroizing::new(
                        rpassword::prompt_password("Slack token: ")
                            .context("cannot read hidden input")?,
                    );
                    secrets::store(&account.token_secret, &token)?;
                }
                SecretCommand::Delete { .. } => secrets::delete(&account.token_secret)?,
            }
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests;
