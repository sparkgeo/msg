use crate::config::SecretRef;
use anyhow::{Context, Result, bail, ensure};
use std::{
    fs::OpenOptions,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
};
use zeroize::Zeroizing;

const MAX_SECRET: u64 = 16 * 1024;

pub fn resolve(reference: &SecretRef) -> Result<Zeroizing<String>> {
    let value = match reference {
        SecretRef::Env { name } => {
            Zeroizing::new(std::env::var(name).map_err(|_| {
                anyhow::anyhow!("secret environment variable is missing or not UTF-8")
            })?)
        }
        SecretRef::File { path } => {
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(path)
                .context("cannot open credential file (must be a regular file, not a symlink)")?;
            let metadata = file.metadata()?;
            ensure!(metadata.is_file(), "credential path must be a regular file");
            ensure!(
                metadata.permissions().mode() & 0o077 == 0,
                "credential file must not grant group/other access; use chmod 600"
            );
            // Root-owned mounted credentials are also accepted when readable by this process.
            // SAFETY: geteuid has no arguments or preconditions and only reads process identity.
            let uid = unsafe { libc::geteuid() };
            ensure!(
                metadata.uid() == uid || metadata.uid() == 0,
                "credential file must be owned by the current user or root"
            );
            let mut bytes = Zeroizing::new(Vec::new());
            file.take(MAX_SECRET + 1)
                .read_to_end(&mut bytes)
                .context("cannot read credential file")?;
            ensure!(
                bytes.len() <= MAX_SECRET as usize,
                "credential file is too large"
            );
            let text = std::str::from_utf8(&bytes).context("credential file must be UTF-8")?;
            // Accommodate a newline from secret provisioning tools, but not arbitrary whitespace.
            Zeroizing::new(text.trim_end_matches(['\r', '\n']).to_owned())
        }
        SecretRef::Keyring { name } => desktop::get(name)?,
    };
    validate(&value)?;
    Ok(value)
}

pub fn validate(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= MAX_SECRET as usize,
        "token must be nonempty and at most 16 KiB"
    );
    ensure!(
        value.bytes().all(|c| c.is_ascii_graphic()),
        "token must contain only printable ASCII without spaces"
    );
    Ok(())
}

pub fn store(reference: &SecretRef, value: &str) -> Result<()> {
    validate(value)?;
    match reference {
        SecretRef::Keyring { name } => desktop::set(name, value),
        _ => bail!(
            "secret set only manages keyring entries; provision file/env credentials outside msg"
        ),
    }
}

pub fn delete(reference: &SecretRef) -> Result<()> {
    match reference {
        SecretRef::Keyring { name } => desktop::delete(name),
        _ => bail!(
            "secret delete only manages keyring entries; manage file/env credentials outside msg"
        ),
    }
}

#[cfg(target_os = "linux")]
mod desktop {
    use super::*;
    use secret_service::{EncryptionType, blocking::SecretService};
    use std::{collections::HashMap, time::Duration};

    fn connect() -> Result<SecretService<'static>> {
        let connection = zbus::blocking::connection::Builder::session()
            .map_err(|_| {
                anyhow::anyhow!("no desktop session bus; use a file/env secret for unattended jobs")
            })?
            .method_timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| anyhow::anyhow!("cannot connect to desktop session bus"))?;
        SecretService::connect_with_existing(EncryptionType::Dh, connection).map_err(|_| {
            anyhow::anyhow!("Secret Service unavailable; use a file/env secret for unattended jobs")
        })
    }

    fn attributes(name: &str) -> HashMap<&str, &str> {
        HashMap::from([("application", "msg"), ("name", name)])
    }

    pub fn get(name: &str) -> Result<Zeroizing<String>> {
        let service = connect()?;
        let items = service
            .search_items(attributes(name))
            .map_err(|_| anyhow::anyhow!("cannot look up keyring entry"))?;
        ensure!(
            items.locked.is_empty(),
            "keyring entry is locked; unlock it outside msg or use a file/env secret"
        );
        ensure!(
            items.unlocked.len() == 1,
            "keyring entry is missing or ambiguous"
        );
        let bytes = Zeroizing::new(
            items.unlocked[0]
                .get_secret()
                .map_err(|_| anyhow::anyhow!("cannot read keyring entry"))?,
        );
        Ok(Zeroizing::new(
            std::str::from_utf8(&bytes)
                .context("keyring token is not UTF-8")?
                .to_owned(),
        ))
    }

    pub fn set(name: &str, value: &str) -> Result<()> {
        let service = connect()?;
        let collection = service.get_default_collection().map_err(|_| {
            anyhow::anyhow!(
                "no default keyring collection; create one in your desktop credential manager"
            )
        })?;
        if collection
            .is_locked()
            .map_err(|_| anyhow::anyhow!("cannot inspect keyring lock"))?
        {
            collection
                .unlock()
                .map_err(|_| anyhow::anyhow!("cannot unlock keyring"))?;
        }
        collection
            .create_item(
                "msg Slack token",
                attributes(name),
                value.as_bytes(),
                true,
                "text/plain",
            )
            .map_err(|_| anyhow::anyhow!("cannot store keyring entry"))?;
        Ok(())
    }

    pub fn delete(name: &str) -> Result<()> {
        let service = connect()?;
        let items = service
            .search_items(attributes(name))
            .map_err(|_| anyhow::anyhow!("cannot look up keyring entry"))?;
        ensure!(
            items.locked.is_empty(),
            "keyring entry is locked; unlock it before deleting"
        );
        ensure!(
            items.unlocked.len() == 1,
            "keyring entry is missing or ambiguous"
        );
        items.unlocked[0]
            .delete()
            .map_err(|_| anyhow::anyhow!("cannot delete keyring entry"))
    }
}

#[cfg(target_os = "macos")]
mod desktop {
    use super::*;
    use security_framework::{os::macos::keychain::SecKeychain, passwords};

    pub fn get(name: &str) -> Result<Zeroizing<String>> {
        let _guard = SecKeychain::disable_user_interaction()
            .map_err(|_| anyhow::anyhow!("cannot disable Keychain prompts"))?;
        let bytes = Zeroizing::new(passwords::get_generic_password("msg", name).map_err(|_| anyhow::anyhow!("Keychain entry unavailable without interaction; unlock/authorize it or use a file/env secret"))?);
        Ok(Zeroizing::new(
            std::str::from_utf8(&bytes)
                .context("Keychain token is not UTF-8")?
                .to_owned(),
        ))
    }
    pub fn set(name: &str, value: &str) -> Result<()> {
        passwords::set_generic_password("msg", name, value.as_bytes())
            .map_err(|_| anyhow::anyhow!("cannot store Keychain entry"))
    }
    pub fn delete(name: &str) -> Result<()> {
        passwords::delete_generic_password("msg", name)
            .map_err(|_| anyhow::anyhow!("cannot delete Keychain entry"))
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("msg currently supports Linux and macOS only");
