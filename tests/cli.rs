use std::{
    fs,
    io::Write,
    process::{Command, Output, Stdio},
};

fn run(home: &std::path::Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_msg"))
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn exact_passthrough_defaults_and_config_roundtrip() {
    let home = tempfile::tempdir().unwrap();
    let input = "\nhello 🦀\r\n  \t\n".as_bytes();
    let output = run(home.path(), &[], input);
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(output.stdout, input);
    assert!(output.stderr.is_empty());
    let output = run(
        home.path(),
        &["config", "set", "timeout_seconds", "20"],
        b"",
    );
    assert!(output.status.success(), "{:?}", output.stderr);
    let output = run(home.path(), &["config", "get", "timeout_seconds"], b"");
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "20");
    let output = run(
        home.path(),
        &[
            "-n",
            "1",
            "--passthrough",
            "--to",
            "stdout",
            "--to",
            "stdout",
        ],
        input,
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, input);
    let output = run(home.path(), &["config", "unset", "timeout_seconds"], b"");
    assert!(output.status.success());
    assert_eq!(run(home.path(), &["hello"], b"").stdout, b"hello\n");
}

#[test]
fn argument_messages_print_like_echo() {
    let home = tempfile::tempdir().unwrap();
    for (args, expected) in [
        (vec!["hello", "world"], "hello world\n"),
        (vec!["hello  world", "🦀"], "hello  world 🦀\n"),
        (vec!["hello\n"], "hello\n\n"),
        (vec![r"hello\nworld"], "hello\\nworld\n"),
        (vec!["--", "-hello", "world"], "-hello world\n"),
        (vec!["--", "config", "list"], "config list\n"),
        (vec!["--", "account", "add"], "account add\n"),
        (
            vec!["-n", "1", "-p", "-t", "stdout", "hello", "world"],
            "hello world\n",
        ),
    ] {
        let output = run(home.path(), &args, b"");
        assert!(output.status.success(), "{args:?}: {:?}", output.stderr);
        assert_eq!(output.stdout, expected.as_bytes(), "{args:?}");
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn account_add_creates_complete_accounts_and_preserves_existing_settings() {
    use std::os::unix::fs::PermissionsExt;

    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config/msg/config.toml");
    let output = run(
        home.path(),
        &["account", "add", "work", "--type", "slack"],
        b"",
    );
    assert!(output.status.success(), "{:?}", output.stderr);
    assert!(output.stdout.is_empty());
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let contents = fs::read_to_string(&path).unwrap();
    let config: toml::Value = contents.parse().unwrap();
    assert_eq!(config["accounts"]["work"]["type"].as_str(), Some("slack"));
    assert_eq!(
        config["accounts"]["work"]["token_secret"]["source"].as_str(),
        Some("keyring")
    );
    assert_eq!(
        config["accounts"]["work"]["token_secret"]["name"].as_str(),
        Some("msg/work/token")
    );
    assert!(config.get("default_slack_account").is_none());
    // Creating a reference does not need a desktop keyring or select a recipient.
    assert_eq!(run(home.path(), &[], b"hello").stdout, b"hello");

    fs::write(
        &path,
        format!("# Keep this comment\ntimeout_seconds = 45\n{contents}"),
    )
    .unwrap();
    let before = fs::read(&path).unwrap();
    let output = run(
        home.path(),
        &["account", "add", "work", "--token-env", "OTHER_TOKEN"],
        b"",
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("account already exists"));
    assert_eq!(fs::read(&path).unwrap(), before);

    let output = run(
        home.path(),
        &["account", "add", "ci", "--token-env", "MSG_CI_TOKEN"],
        b"",
    );
    assert!(output.status.success(), "{:?}", output.stderr);
    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("# Keep this comment"));
    let config: toml::Value = contents.parse().unwrap();
    assert_eq!(config["timeout_seconds"].as_integer(), Some(45));
    assert_eq!(
        config["accounts"]["ci"]["token_secret"]["source"].as_str(),
        Some("env")
    );
    assert_eq!(
        config["accounts"]["ci"]["token_secret"]["name"].as_str(),
        Some("MSG_CI_TOKEN")
    );
    assert_eq!(
        config["accounts"]["work"]["token_secret"]["name"].as_str(),
        Some("msg/work/token")
    );
}

#[test]
fn account_add_supports_explicit_paths_files_and_inline_accounts() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("nested/custom.toml");
    let config = path.to_str().unwrap();
    let output = run(
        home.path(),
        &[
            "account",
            "add",
            "job",
            "--token-file",
            "/not-yet-provisioned/slack token",
            "--config",
            config,
        ],
        b"",
    );
    assert!(output.status.success(), "{:?}", output.stderr);
    let value: toml::Value = fs::read_to_string(&path).unwrap().parse().unwrap();
    assert_eq!(
        value["accounts"]["job"]["token_secret"]["source"].as_str(),
        Some("file")
    );
    assert_eq!(
        value["accounts"]["job"]["token_secret"]["path"].as_str(),
        Some("/not-yet-provisioned/slack token")
    );
    assert!(!home.path().join("config/msg/config.toml").exists());

    fs::write(&path, "accounts = {} # Keep inline accounts\n").unwrap();
    let output = run(
        home.path(),
        &["--config", config, "account", "add", "work"],
        b"",
    );
    assert!(output.status.success(), "{:?}", output.stderr);
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains("# Keep inline accounts")
    );
    let output = run(
        home.path(),
        &["--config", config, "config", "get", "accounts.work.type"],
        b"",
    );
    assert_eq!(output.stdout, b"slack\n");
}

#[test]
fn invalid_account_setup_does_not_write_configuration() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config/msg/config.toml");
    for args in [
        vec!["account", "add", "bad.name"],
        vec!["account", "add", ""],
        vec!["account", "add", "work", "--type", "email"],
        vec!["account", "add", "work", "--token-env", ""],
        vec!["account", "add", "work", "--token-env", "bad\nname"],
        vec!["account", "add", "work", "--token-file", "relative/path"],
        vec![
            "account",
            "add",
            "work",
            "--token-file",
            "/token",
            "--token-env",
            "TOKEN",
        ],
        vec!["--to", "stdout", "account", "add", "work"],
        vec!["--account", "work", "account", "add", "work"],
    ] {
        let output = run(home.path(), &args, b"");
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        assert!(!path.exists(), "{args:?}");
    }
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "invalid config PRIVATE_VALUE").unwrap();
    let output = run(home.path(), &["account", "add", "work"], b"");
    assert_eq!(output.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_VALUE"));
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "invalid config PRIVATE_VALUE"
    );
}

#[test]
fn config_set_uses_setting_types_and_roundtrips_literal_strings() {
    let home = tempfile::tempdir().unwrap();
    assert!(
        run(home.path(), &["account", "add", "123"], b"")
            .status
            .success()
    );
    for name in ["123", "true", "2026-10-23"] {
        let output = run(
            home.path(),
            &["config", "set", "default_slack_account", name],
            b"",
        );
        assert!(output.status.success(), "{:?}", output.stderr);
        assert_eq!(
            run(
                home.path(),
                &["config", "get", "default_slack_account"],
                b""
            )
            .stdout,
            format!("{name}\n").as_bytes()
        );
    }
    let key = "accounts.123.token_secret.name";
    for value in [
        "123",
        "false",
        "[entry]",
        "{entry}",
        "\"quoted\"",
        "'quoted'",
        r"path\with\slashes",
        "name with spaces",
    ] {
        let output = run(home.path(), &["config", "set", key, value], b"");
        assert!(output.status.success(), "{value:?}: {:?}", output.stderr);
        assert_eq!(
            run(home.path(), &["config", "get", key], b"").stdout,
            format!("{value}\n").as_bytes()
        );
    }
    let path = home.path().join("config/msg/config.toml");
    let before = fs::read(&path).unwrap();
    for value in ["0", "301", "1.5", "true", "\"20\"", "PRIVATE_VALUE"] {
        let output = run(
            home.path(),
            &["config", "set", "timeout_seconds", value],
            b"",
        );
        assert_eq!(output.status.code(), Some(2));
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("timeout_seconds must be an integer between 1 and 300"));
        assert!(!error.contains("PRIVATE_VALUE"));
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn no_newline_suppresses_only_the_added_stdout_newline() {
    let home = tempfile::tempdir().unwrap();
    for (args, input, expected) in [
        (vec!["--no-newline", "hello", "world"], "", "hello world"),
        (vec!["--no-newline", "hello\n"], "", "hello\n"),
        (vec!["--no-newline"], "hello\r\n\n", "hello\r\n\n"),
        (
            vec!["--no-newline", "-n", "1", "-p", "-t", "stdout", "hello"],
            "",
            "hello",
        ),
    ] {
        let output = run(home.path(), &args, input.as_bytes());
        assert!(output.status.success(), "{args:?}: {:?}", output.stderr);
        assert_eq!(output.stdout, expected.as_bytes(), "{args:?}");
        assert!(output.stderr.is_empty());
    }
    let output = run(home.path(), &["--no-newline", "config", "list"], b"");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn removed_recipient_settings_and_aliases_are_rejected() {
    let home = tempfile::tempdir().unwrap();
    for (key, value) in [
        ("default_outputs", "[\"slack:#general\"]"),
        ("default_destination", "team"),
        (
            "destinations.team",
            "{ account = \"work\", channel = \"C123\" }",
        ),
    ] {
        let output = run(home.path(), &["config", "set", key, value], b"");
        assert_eq!(output.status.code(), Some(2));
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("configured recipients are no longer supported")
        );
    }
    // Supplying an unprefixed alias never silently falls back to stdout.
    let output = run(home.path(), &["--to", "stdout", "--to", "team"], b"hello");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(run(home.path(), &[], b"hello").stdout, b"hello");
}

#[test]
fn invalid_input_and_invalid_config_do_not_leak_content() {
    let home = tempfile::tempdir().unwrap();
    for (args, input) in [
        (vec![], vec![]),
        (vec![], vec![255]),
        (vec!["argument"], b"stdin".to_vec()),
        (vec!["-n", "0"], b"hi".to_vec()),
    ] {
        let output = run(home.path(), &args, &input);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
    let path = home.path().join("bad.toml");
    fs::write(&path, "token = 'SUPER_PRIVATE_VALUE'\nbroken syntax").unwrap();
    let output = run(home.path(), &["--config", path.to_str().unwrap()], b"hi");
    assert_eq!(output.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("SUPER_PRIVATE_VALUE"));
}

#[test]
fn structured_report_keeps_stdout_clean_and_refuses_overwrite() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("result.json");
    let output = run(
        home.path(),
        &["--result-file", path.to_str().unwrap()],
        b"message\n",
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"message\n");
    let report: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(report[0]["destination"], "stdout");
    assert_eq!(report[0]["status"], "written");
    let output = run(
        home.path(),
        &["--result-file", path.to_str().unwrap()],
        b"again",
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn explicit_config_works_with_subcommands_and_account_setup() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("custom.toml");
    let config = path.to_str().unwrap();
    let output = run(
        home.path(),
        &[
            "--config",
            config,
            "config",
            "set",
            "accounts.work",
            "{ type = \"slack\", token_secret = { source = \"env\", name = \"MSG_TEST_TOKEN\" } }",
        ],
        b"",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = run(
        home.path(),
        &[
            "config",
            "set",
            "accounts.work.token_secret.name",
            "MSG_NEW_TOKEN",
            "--config",
            config,
        ],
        b"",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = run(
        home.path(),
        &[
            "--config",
            config,
            "config",
            "get",
            "accounts.work.token_secret.name",
        ],
        b"",
    );
    assert_eq!(output.stdout, b"MSG_NEW_TOKEN\n");
    assert!(
        run(
            home.path(),
            &["--config", config, "--to", "stdout"],
            b"hello"
        )
        .status
        .success()
    );
    assert_eq!(
        run(home.path(), &["--to", "stdout", "config", "list"], b"")
            .status
            .code(),
        Some(2)
    );
    assert!(
        !run(
            home.path(),
            &[
                "--config",
                config,
                "config",
                "set",
                "accounts.work.type",
                "email"
            ],
            b""
        )
        .status
        .success()
    );
    // A rejected edit must leave the previous configuration intact.
    assert_eq!(
        run(
            home.path(),
            &["--config", config, "config", "get", "accounts.work.type"],
            b""
        )
        .stdout,
        b"slack\n"
    );
}

#[test]
fn explicit_recipients_need_an_account_but_plain_stdout_does_not() {
    let home = tempfile::tempdir().unwrap();
    let output = run(
        home.path(),
        &[
            "--to",
            "stdout",
            "--to",
            "slack:#my-channel",
            "--to",
            "slack:@dustin",
        ],
        b"hi",
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("configure a Slack account"));
    assert!(
        run(
            home.path(),
            &["config", "set", "default_slack_account", "work"],
            b""
        )
        .status
        .success()
    );
    // Choosing credentials in config never chooses a recipient or resolves a token.
    assert_eq!(run(home.path(), &[], b"hi").stdout, b"hi");
    assert_eq!(
        run(home.path(), &["--account", "work", "config", "list"], b"")
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn legacy_config_cleanup_preserves_accounts_and_comments() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("old.toml");
    fs::write(
        &path,
        r#"default_outputs = ["team"]
default_destination = "team"
[destinations.team]
account = "work"
channel = "C123"
# Keep my account
[accounts.work]
type = "slack"
token_secret = { source = "env", name = "MSG_MISSING_TEST_TOKEN" }
"#,
    )
    .unwrap();
    let path_arg = path.to_str().unwrap();
    let output = run(home.path(), &["--config", path_arg], b"hello");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("msg config unset default_outputs"));
    for key in ["default_outputs", "destinations", "default_destination"] {
        let output = run(
            home.path(),
            &["--config", path_arg, "config", "unset", key],
            b"",
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("# Keep my account"));
    assert!(contents.contains("MSG_MISSING_TEST_TOKEN"));
    assert!(!contents.contains("destinations"));
    assert!(!contents.contains("default_outputs"));
    let output = run(home.path(), &["--config", path_arg, "config", "list"], b"");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("[accounts.work.token_secret]"));
    // The preserved token is never read until Slack is explicitly requested.
    let output = run(home.path(), &["--config", path_arg], b"full input\n");
    assert!(output.status.success());
    assert_eq!(output.stdout, b"full input\n");
}
