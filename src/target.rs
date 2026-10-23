use anyhow::{Result, bail, ensure};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SlackTarget {
    ChannelId(String),
    UserId(String),
    ChannelName(String),
    Username(String),
}

pub enum Target {
    Stdout,
    Slack(SlackTarget),
}

pub fn parse(value: &str) -> Result<Target> {
    if value == "stdout" {
        return Ok(Target::Stdout);
    }
    if let Some((service, recipient)) = value.split_once(':') {
        ensure!(
            service == "slack",
            "unsupported service prefix; use slack:#channel or slack:@username"
        );
        let target = if let Some(name) = recipient.strip_prefix('#') {
            ensure!(valid_handle(name), "invalid Slack channel name");
            SlackTarget::ChannelName(name.into())
        } else if let Some(name) = recipient.strip_prefix('@') {
            ensure!(valid_handle(name), "invalid Slack username");
            SlackTarget::Username(name.into())
        } else if valid_id(recipient, b"CGD") {
            SlackTarget::ChannelId(recipient.into())
        } else if valid_id(recipient, b"UW") {
            SlackTarget::UserId(recipient.into())
        } else {
            bail!("use slack:#channel, slack:@username, or slack:SLACK_ID");
        };
        return Ok(Target::Slack(target));
    }
    bail!(
        "use --to stdout, --to slack:#channel, --to slack:@username, or --to slack:SLACK_ID; named destinations are no longer supported"
    )
}

fn valid_handle(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

pub fn valid_id(value: &str, prefixes: &[u8]) -> bool {
    value.len() > 1
        && prefixes.contains(&value.as_bytes()[0])
        && value
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}
