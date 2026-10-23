use crate::{
    cli::SendArgs,
    config::{Config, MAX_INPUT_BYTES, valid_name},
    secrets,
    slack::{self, Failure, Slack, Status},
    target::{self, SlackTarget, Target},
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashSet},
    io::{Read, Write},
};
use zeroize::Zeroizing;

pub fn read_input(message: Option<String>, mut input: impl Read, terminal: bool) -> Result<String> {
    if let Some(message) = message {
        ensure!(!message.is_empty(), "message is empty");
        ensure!(
            message.len() <= MAX_INPUT_BYTES,
            "message exceeds the 8 MiB input limit"
        );
        if !terminal {
            let mut byte = [0];
            ensure!(
                input.read(&mut byte)? == 0,
                "choose either a message argument or stdin, not both"
            );
        }
        return Ok(message);
    }
    ensure!(
        !terminal,
        "provide a message argument or pipe content to msg; see --help"
    );
    let mut bytes = Vec::new();
    input
        .take(MAX_INPUT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .context("cannot read stdin")?;
    ensure!(
        bytes.len() <= MAX_INPUT_BYTES,
        "input exceeds the 8 MiB limit"
    );
    ensure!(!bytes.is_empty(), "message is empty");
    String::from_utf8(bytes).map_err(|_| anyhow::anyhow!("message must be valid UTF-8"))
}

struct Destination {
    label: String,
    slack: Option<(String, SlackTarget)>,
}

pub struct Plan {
    destinations: Vec<Destination>,
    tokens: BTreeMap<String, Zeroizing<String>>,
    original: String,
    stdout_newline: bool,
    slack_text: String,
}

impl Plan {
    pub fn prepare(config: &Config, args: &SendArgs, original: String) -> Result<Self> {
        ensure!(args.max_chars != Some(0), "-n must be greater than zero");
        if let Some(account) = &args.account {
            ensure!(
                valid_name(account) && config.accounts.contains_key(account),
                "--account must name a configured Slack account"
            );
        }
        let mut labels = if args.destinations.is_empty() {
            vec!["stdout".into()]
        } else {
            args.destinations.clone()
        };
        if args.passthrough {
            labels.insert(0, "stdout".into());
        }
        let mut seen = HashSet::new();
        labels.retain(|name| seen.insert(name.clone()));
        let mut destinations = Vec::new();
        let mut tokens = BTreeMap::new();
        let limited: String = original
            .chars()
            .take(args.max_chars.unwrap_or(usize::MAX))
            .collect();
        let slack_text = slack::plain_text(&limited);
        for label in labels {
            let route = match target::parse(&label)? {
                Target::Stdout => None,
                Target::Slack(target) => {
                    let account = if let Some(account) = args
                        .account
                        .as_ref()
                        .or(config.default_slack_account.as_ref())
                    {
                        account.clone()
                    } else {
                        ensure!(
                            !config.accounts.is_empty(),
                            "configure a Slack account with msg account add NAME before using slack: destinations"
                        );
                        ensure!(
                            config.accounts.len() == 1,
                            "multiple Slack accounts configured; use --account NAME or set default_slack_account"
                        );
                        config.accounts.keys().next().unwrap().clone()
                    };
                    Some((account, target))
                }
            };
            if let Some((account_name, _)) = &route {
                ensure!(
                    slack_text.chars().count() <= slack::MAX_SLACK_CHARS,
                    "Slack message exceeds 40000 characters after escaping; use -n (stdout remains complete)"
                );
                let account = config
                    .accounts
                    .get(account_name)
                    .context("destination references an unknown Slack account")?;
                if !tokens.contains_key(account_name) {
                    tokens.insert(
                        account_name.clone(),
                        secrets::resolve(&account.token_secret).with_context(|| {
                            format!("cannot resolve token for account '{account_name}'")
                        })?,
                    );
                }
            }
            destinations.push(Destination {
                label,
                slack: route,
            });
        }
        Ok(Self {
            destinations,
            tokens,
            original,
            stdout_newline: !args.message.is_empty() && !args.no_newline,
            slack_text,
        })
    }

    pub fn needs_slack(&self) -> bool {
        !self.tokens.is_empty()
    }

    pub fn execute(&self, slack: Option<&Slack>, output: &mut impl Write) -> Result<Vec<Outcome>> {
        // Complete all name lookups before any message is sent or stdout is written.
        let mut cache: std::collections::HashMap<(String, SlackTarget), SlackTarget> =
            std::collections::HashMap::new();
        let mut resolved = Vec::new();
        for destination in &self.destinations {
            let route = if let Some((account, target)) = &destination.slack {
                let key = (account.clone(), target.clone());
                let target = if let Some(target) = cache.get(&key) {
                    target.clone()
                } else {
                    let target = match target {
                        SlackTarget::ChannelName(name) => SlackTarget::ChannelId(
                            slack
                                .context("Slack client unavailable")?
                                .lookup(&self.tokens[account], name, false)
                                .with_context(|| {
                                    format!(
                                        "cannot resolve '{}' (no messages sent)",
                                        destination.label
                                    )
                                })?,
                        ),
                        SlackTarget::Username(name) => SlackTarget::UserId(
                            slack
                                .context("Slack client unavailable")?
                                .lookup(&self.tokens[account], name, true)
                                .with_context(|| {
                                    format!(
                                        "cannot resolve '{}' (no messages sent)",
                                        destination.label
                                    )
                                })?,
                        ),
                        target => target.clone(),
                    };
                    cache.insert(key, target.clone());
                    target
                };
                Some((account, target))
            } else {
                None
            };
            resolved.push((destination, route));
        }
        Ok(resolved
            .into_iter()
            .map(|(destination, route)| {
                let result = if let Some((account, target)) = route {
                    let (channel, user) = match &target {
                        SlackTarget::ChannelId(id) => (Some(id.as_str()), None),
                        SlackTarget::UserId(id) => (None, Some(id.as_str())),
                        _ => unreachable!("all names resolved before delivery"),
                    };
                    slack
                        .expect("Slack client prepared before delivery")
                        .send(&self.tokens[account], channel, user, &self.slack_text)
                        .map(|_| Status::Accepted)
                } else {
                    output
                        .write_all(self.original.as_bytes())
                        .and_then(|_| {
                            if self.stdout_newline {
                                output.write_all(b"\n")
                            } else {
                                Ok(())
                            }
                        })
                        .and_then(|_| output.flush())
                        .map(|_| Status::Written)
                        .map_err(|_| {
                            Failure::failed("cannot write stdout; output may be incomplete")
                        })
                };
                match result {
                    Ok(status) => Outcome {
                        destination: destination.label.clone(),
                        status,
                        error: None,
                    },
                    Err(error) => Outcome {
                        destination: destination.label.clone(),
                        status: error.status,
                        error: Some(error.message),
                    },
                }
            })
            .collect())
    }
}

#[derive(Serialize, Debug)]
pub struct Outcome {
    pub destination: String,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn exit_code(outcomes: &[Outcome]) -> u8 {
    let success = outcomes
        .iter()
        .filter(|outcome| matches!(outcome.status, Status::Accepted | Status::Written))
        .count();
    if success == outcomes.len() {
        0
    } else if success == 0 {
        3
    } else {
        4
    }
}
