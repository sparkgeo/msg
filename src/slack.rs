use anyhow::{Context, Result, bail, ensure};
use reqwest::{blocking::Client, redirect::Policy};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    io::Read,
    time::{Duration, Instant},
};

pub const MAX_SLACK_CHARS: usize = 40_000;

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Accepted,
    Written,
    Failed,
    Uncertain,
}

#[derive(Debug)]
pub struct Failure {
    pub status: Status,
    pub message: String,
}

impl Failure {
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            status: Status::Failed,
            message: message.into(),
        }
    }
    fn uncertain(message: impl Into<String>) -> Self {
        Self {
            status: Status::Uncertain,
            message: message.into(),
        }
    }
}

pub struct Slack {
    client: Client,
    endpoint: String,
    timeout: Duration,
}

impl Slack {
    #[cfg(test)]
    pub fn local_test(endpoint: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(2))
                .no_proxy()
                .redirect(Policy::none())
                .retry(reqwest::retry::never())
                .build()
                .unwrap(),
            endpoint,
            timeout: Duration::from_secs(2),
        }
    }

    pub fn new(timeout: u64) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(timeout))
                .connect_timeout(Duration::from_secs(timeout.min(10)))
                .redirect(Policy::none())
                .retry(reqwest::retry::never())
                .user_agent(concat!("msg/", env!("CARGO_PKG_VERSION")))
                .build()
                .context("cannot initialize HTTPS client")?,
            endpoint: "https://slack.com/api".into(),
            timeout: Duration::from_secs(timeout),
        })
    }

    fn request(
        &self,
        method: &str,
        token: &str,
        body: Value,
    ) -> std::result::Result<Value, Failure> {
        self.request_with_timeout(method, token, body, self.timeout)
    }

    fn request_with_timeout(
        &self,
        method: &str,
        token: &str,
        body: Value,
        timeout: Duration,
    ) -> std::result::Result<Value, Failure> {
        let url = format!("{}/{method}", self.endpoint);
        let request = if matches!(method, "conversations.list" | "users.list") {
            self.client.get(url).query(&body)
        } else {
            self.client.post(url).json(&body)
        };
        let response = request
            .bearer_auth(token)
            .timeout(timeout)
            .send()
            .map_err(|error| {
                if error.is_connect() || error.is_builder() {
                    Failure::failed("cannot connect to Slack; check network and proxy settings")
                } else {
                    Failure::uncertain(
                        "Slack request did not complete; acceptance is unknown (not retried)",
                    )
                }
            })?;
        let code = response.status();
        if code.as_u16() == 429 {
            let retry = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());
            return Err(Failure::failed(match retry {
                Some(seconds) => format!("Slack rate limit; retry after {seconds} seconds"),
                None => "Slack rate limit; retry later".into(),
            }));
        }
        if code.is_server_error() {
            return Err(Failure::uncertain(format!(
                "Slack returned HTTP {}; acceptance is unknown",
                code.as_u16()
            )));
        }
        if !code.is_success() {
            return Err(Failure::failed(format!(
                "Slack returned HTTP {}",
                code.as_u16()
            )));
        }
        let mut bytes = Vec::new();
        response
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Failure::uncertain("cannot read Slack response; acceptance is unknown"))?;
        if bytes.len() > 1024 * 1024 {
            return Err(Failure::uncertain(
                "Slack response too large; acceptance is unknown",
            ));
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Failure::uncertain("invalid Slack response; acceptance is unknown"))?;
        match value.get("ok").and_then(Value::as_bool) {
            Some(true) => Ok(value),
            Some(false) => {
                // Never echo arbitrary server text, which could contain credentials or message data.
                let explanation = match value.get("error").and_then(Value::as_str).unwrap_or("") {
                    "invalid_auth" | "not_authed" | "token_revoked" | "account_inactive" => {
                        "Slack authentication failed; check the account token"
                    }
                    "missing_scope" => "Slack token lacks a required scope",
                    "channel_not_found" | "not_in_channel" | "user_not_found" => {
                        "Slack destination unavailable; check IDs and bot membership"
                    }
                    "is_archived" => "Slack channel is archived",
                    "msg_too_long" => "Slack rejected the message length; use -n",
                    "ratelimited" => "Slack rate limit; retry later",
                    _ => "Slack rejected the request",
                };
                Err(Failure::failed(explanation))
            }
            None => Err(Failure::uncertain(
                "Slack response has no acceptance status",
            )),
        }
    }

    /// Resolve an exact username or channel name over every page. No display-name guesses.
    pub fn lookup(&self, token: &str, name: &str, user: bool) -> Result<String> {
        let (method, field, prefixes, scopes) = if user {
            ("users.list", "members", b"UW".as_slice(), "users:read")
        } else {
            (
                "conversations.list",
                "channels",
                b"CG".as_slice(),
                "channels:read for public channels and groups:read for private channels",
            )
        };
        let start = Instant::now();
        let mut cursor = String::new();
        let mut cursors = HashSet::new();
        let mut matches = HashSet::new();
        for _ in 0..1000 {
            let remaining = self.timeout.checked_sub(start.elapsed()).filter(|time| !time.is_zero())
                .context("Slack name lookup exceeded timeout_seconds; use a Slack ID or increase the timeout")?;
            let mut query = json!({"limit": 200});
            if !user {
                query["types"] = json!("public_channel,private_channel");
                query["exclude_archived"] = json!(true);
            }
            if !cursor.is_empty() {
                query["cursor"] = json!(cursor);
            }
            let page = self
                .request_with_timeout(method, token, query, remaining)
                .map_err(|failure| {
                    let detail = if failure.status == Status::Uncertain {
                        "lookup response incomplete or invalid".to_owned()
                    } else {
                        failure.message
                    };
                    anyhow::anyhow!("Slack name lookup failed: {detail}; lookup requires {scopes}")
                })?;
            let entries = page
                .get(field)
                .and_then(Value::as_array)
                .context("Slack name lookup returned an invalid list")?;
            for entry in entries {
                if entry.get("name").and_then(Value::as_str) != Some(name)
                    || entry
                        .get(if user { "deleted" } else { "is_archived" })
                        .and_then(Value::as_bool)
                        == Some(true)
                {
                    continue;
                }
                let id = entry
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|id| crate::target::valid_id(id, prefixes))
                    .context("Slack name lookup returned an invalid ID")?;
                matches.insert(id.to_owned());
            }
            cursor = match page.pointer("/response_metadata/next_cursor") {
                None => String::new(),
                Some(value) => value
                    .as_str()
                    .context("Slack returned an invalid pagination cursor")?
                    .trim()
                    .to_owned(),
            };
            if cursor.is_empty() {
                ensure!(
                    !matches.is_empty(),
                    "Slack name not found or not accessible; use the exact {} and check {scopes}",
                    if user {
                        "username (not display name)"
                    } else {
                        "channel name and bot membership"
                    }
                );
                ensure!(
                    matches.len() == 1,
                    "Slack name is ambiguous; use slack:SLACK_ID"
                );
                return Ok(matches.into_iter().next().unwrap());
            }
            ensure!(
                cursors.insert(cursor.clone()),
                "Slack repeated a pagination cursor; use slack:SLACK_ID"
            );
        }
        bail!("Slack name lookup exceeded the page limit; use slack:SLACK_ID")
    }

    pub fn send(
        &self,
        token: &str,
        channel: Option<&str>,
        user: Option<&str>,
        text: &str,
    ) -> std::result::Result<(), Failure> {
        let channel = if let Some(user) = user {
            let response = self.request("conversations.open", token, json!({"users": user}))
                .map_err(|_| Failure::failed("cannot open Slack DM; check user ID, im:write scope, and connectivity (message not sent)"))?;
            response
                .pointer("/channel/id")
                .and_then(Value::as_str)
                .filter(|s| s.starts_with('D'))
                .ok_or_else(|| Failure::failed("Slack did not return a DM ID; message not sent"))?
                .to_owned()
        } else {
            channel.unwrap_or_default().to_owned()
        };
        self.request(
            "chat.postMessage",
            token,
            json!({
                "channel": channel, "text": text, "mrkdwn": false,
                "parse": "none", "link_names": false, "unfurl_links": false, "unfurl_media": false,
            }),
        )?;
        Ok(())
    }
}

pub fn plain_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
