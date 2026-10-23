use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
};
use toml_edit::{DocumentMut, Item, Table, TableLike};

pub const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
const REMOVED_RECIPIENT_KEYS: &[&str] = &["default_outputs", "default_destination", "destinations"];

#[derive(Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_slack_account: Option<String>,
    pub timeout_seconds: u64,
    pub accounts: BTreeMap<String, Account>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_slack_account: None,
            timeout_seconds: 30,
            accounts: BTreeMap::new(),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
    #[serde(rename = "type")]
    pub kind: AccountType,
    pub token_secret: SecretRef,
}

#[derive(Clone, Deserialize, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum AccountType {
    Slack,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "source", rename_all = "lowercase", deny_unknown_fields)]
pub enum SecretRef {
    Env { name: String },
    File { path: PathBuf },
    Keyring { name: String },
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=300).contains(&self.timeout_seconds),
            "timeout_seconds must be between 1 and 300"
        );
        for name in self.accounts.keys() {
            ensure!(
                valid_name(name),
                "names must contain only letters, digits, underscores, and hyphens"
            );
        }
        if let Some(account) = &self.default_slack_account {
            ensure!(valid_name(account), "invalid default_slack_account name");
        }
        for account in self.accounts.values() {
            match &account.token_secret {
                SecretRef::Env { name } | SecretRef::Keyring { name } => ensure!(
                    !name.is_empty() && !name.chars().any(char::is_control),
                    "secret names must be nonempty and contain no control characters"
                ),
                SecretRef::File { path } => {
                    ensure!(path.is_absolute(), "credential file paths must be absolute")
                }
            }
        }
        Ok(())
    }
}

pub fn config_path(explicit: Option<PathBuf>) -> Result<PathBuf> {
    let path = if let Some(path) = explicit {
        path
    } else {
        match std::env::var_os("XDG_CONFIG_HOME")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
        {
            Some(base) if base.is_absolute() => base.join("msg/config.toml"),
            _ => PathBuf::from(
                std::env::var_os("HOME")
                    .filter(|s| !s.is_empty())
                    .context("HOME is not set; use --config PATH")?,
            )
            .join(".config/msg/config.toml"),
        }
    };
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

pub fn read_document(path: &Path, required: bool) -> Result<DocumentMut> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound && !required => String::new(),
        Err(err) => return Err(err).context("cannot read configuration file"),
    };
    // Parser diagnostics can contain source lines (including accidentally pasted secrets).
    text.parse().map_err(|_| {
        anyhow::anyhow!(
            "invalid TOML configuration; check syntax (source omitted to protect secrets)"
        )
    })
}

pub fn decode(doc: &DocumentMut) -> Result<Config> {
    for key in REMOVED_RECIPIENT_KEYS {
        ensure!(
            !doc.contains_key(key),
            "configured recipients are no longer supported; run msg config unset {key} (with --config PATH if needed), then pass recipients with --to"
        );
    }
    let config: Config = toml::from_str(&doc.to_string()).map_err(|_| anyhow::anyhow!("invalid configuration schema; see config.example.toml (source omitted to protect secrets)"))?;
    config.validate()?;
    Ok(config)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("output file requires a parent directory")?;
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .context("cannot create parent directory")?;
    let mut temp =
        tempfile::NamedTempFile::new_in(parent).context("cannot create temporary file")?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path)
        .map_err(|e| e.error)
        .context("cannot replace output file")?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

pub fn modify(doc: &mut DocumentMut, key: &str, value: Option<&str>) -> Result<()> {
    let keys: Vec<_> = key.split('.').collect();
    ensure!(
        keys.iter().all(|key| valid_name(key)),
        "invalid dotted configuration key"
    );
    let mut table: &mut dyn TableLike = doc.as_table_mut();
    for key in &keys[..keys.len() - 1] {
        if !table.contains_key(key) {
            ensure!(value.is_some(), "setting does not exist");
            let mut child = Table::new();
            child.set_implicit(true);
            table.insert(key, Item::Table(child));
        }
        table = table
            .get_mut(key)
            .and_then(Item::as_table_like_mut)
            .context("parent setting is not a table")?;
    }
    let key = keys.last().unwrap();
    if let Some(raw) = value {
        let parsed = setting_value(&keys, raw)?;
        table.insert(key, Item::Value(parsed));
    } else {
        ensure!(table.remove(key).is_some(), "setting does not exist");
    }
    if value.is_none() {
        // Permit removing obsolete settings one at a time without touching accounts.
        let mut current = doc.clone();
        for key in REMOVED_RECIPIENT_KEYS {
            current.remove(key);
        }
        decode(&current)?;
    } else {
        decode(doc)?;
    }
    Ok(())
}

fn setting_value(keys: &[&str], raw: &str) -> Result<toml_edit::Value> {
    match keys {
        ["timeout_seconds"] => {
            let seconds = raw
                .parse::<i64>()
                .ok()
                .filter(|seconds| (1..=300).contains(seconds))
                .context("timeout_seconds must be an integer between 1 and 300")?;
            Ok(toml_edit::Value::from(seconds))
        }
        ["default_slack_account"]
        | ["accounts", _, "type"]
        | ["accounts", _, "token_secret", "source" | "name" | "path"] => {
            Ok(toml_edit::Value::from(raw))
        }
        // Keep structured edits available for replacing whole accounts or secret references.
        _ => match raw.parse::<toml_edit::Value>() {
            Ok(value) => Ok(value),
            Err(_) if !raw.starts_with(['[', '{', '"', '\'']) => Ok(toml_edit::Value::from(raw)),
            Err(_) => bail!("invalid TOML value"),
        },
    }
}

pub fn add_account(doc: &mut DocumentMut, name: &str, account: Account) -> Result<()> {
    ensure!(
        valid_name(name),
        "names must contain only letters, digits, underscores, and hyphens"
    );
    let current = decode(doc)?;
    ensure!(
        !current.accounts.contains_key(name),
        "account already exists; use msg config set to update it"
    );
    // Work on a copy so an invalid account never leaves a partial account behind.
    let mut updated = doc.clone();
    if !updated.contains_key("accounts") {
        let mut accounts = Table::new();
        accounts.set_implicit(true);
        updated.insert("accounts", Item::Table(accounts));
    }
    let accounts = updated
        .get_mut("accounts")
        .and_then(Item::as_table_like_mut)
        .context("accounts must be a table")?;
    accounts.insert(
        name,
        Item::Table(toml_edit::ser::to_document(&account)?.into_table()),
    );
    decode(&updated)?;
    *doc = updated;
    Ok(())
}

pub fn get(config: &Config, key: &str) -> Result<String> {
    let root = toml::Value::try_from(config)?;
    let mut value = &root;
    for key in key.split('.') {
        value = value.get(key).context("setting does not exist")?;
    }
    Ok(match value {
        toml::Value::String(text) => text.clone(),
        _ => value.to_string(),
    })
}
