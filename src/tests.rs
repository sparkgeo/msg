use super::*;
use crate::{
    cli::SendArgs,
    config::{Config, SecretRef},
    delivery::{Plan, exit_code},
    slack::{Slack, Status},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Cursor, Read, Write},
    net::TcpListener,
    os::unix::fs::{PermissionsExt, symlink},
    thread,
    time::{Duration, Instant},
};

fn args(names: &[&str]) -> SendArgs {
    SendArgs {
        destinations: names.iter().map(|s| (*s).into()).collect(),
        account: None,
        passthrough: false,
        no_newline: false,
        max_chars: None,
        result_file: None,
        message: Vec::new(),
    }
}

fn fixture() -> (tempfile::TempDir, Config) {
    let directory = tempfile::tempdir().unwrap();
    let token = directory.path().join("token");
    fs::write(&token, "fake-test-token\n").unwrap();
    fs::set_permissions(&token, fs::Permissions::from_mode(0o600)).unwrap();
    let config = format!(
        r#"
        [accounts.work]
        type = "slack"
        token_secret = {{ source = "file", path = {:?} }}
    "#,
        token
    );
    (directory, config::decode(&config.parse().unwrap()).unwrap())
}

struct Request {
    path: String,
    body: Value,
    authorized: bool,
}

fn server(responses: Vec<(u16, Value, Option<u64>)>) -> (Slack, thread::JoinHandle<Vec<Request>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, body, retry) in responses {
            let start = Instant::now();
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == io::ErrorKind::WouldBlock
                            && start.elapsed() < Duration::from_secs(5) =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("mock did not receive expected request: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = Vec::new();
            let header_end = loop {
                let mut buffer = [0u8; 1024];
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
                if let Some(index) = bytes.windows(4).position(|p| p == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|s| s.parse::<usize>().ok())
                })
                .unwrap_or(0);
            while bytes.len() < header_end + length {
                let mut buffer = [0u8; 1024];
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
            }
            let path = headers.split_whitespace().nth(1).unwrap().to_owned();
            let url = reqwest::Url::parse(&format!("http://localhost{path}")).unwrap();
            let request_body = if length == 0 {
                Value::Object(
                    url.query_pairs()
                        .map(|(key, value)| (key.into_owned(), Value::String(value.into_owned())))
                        .collect(),
                )
            } else {
                serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap()
            };
            requests.push(Request {
                path: url.path().to_owned(),
                authorized: headers
                    .to_ascii_lowercase()
                    .contains("authorization: bearer fake-test-token"),
                body: request_body,
            });
            let body = body.to_string();
            let retry = retry
                .map(|s| format!("Retry-After: {s}\r\n"))
                .unwrap_or_default();
            write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{retry}\r\n{body}", body.len()).unwrap();
        }
        requests
    });
    (Slack::local_test(endpoint), handle)
}

#[test]
fn input_preserves_unicode_whitespace_and_rejects_bad_input() {
    let text = "\n  hello 🦀\r\n\t\n";
    assert_eq!(
        delivery::read_input(None, Cursor::new(text), false).unwrap(),
        text
    );
    assert!(delivery::read_input(None, Cursor::new([0xff]), false).is_err());
    assert!(delivery::read_input(None, Cursor::new([]), false).is_err());
    assert!(delivery::read_input(None, Cursor::new([]), true).is_err());
    assert!(delivery::read_input(Some("a".into()), Cursor::new(b"b"), false).is_err());
    assert_eq!(
        delivery::read_input(Some("a".into()), Cursor::new([]), false).unwrap(),
        "a"
    );
    assert!(
        delivery::read_input(
            None,
            Cursor::new(vec![b'a'; config::MAX_INPUT_BYTES + 1]),
            false
        )
        .is_err()
    );
}

#[test]
fn stdout_default_deduplication_and_limit_preserve_full_message() {
    let config = Config::default();
    let mut options = args(&["stdout", "stdout"]);
    options.passthrough = true;
    options.max_chars = Some(2);
    let input = "hello 🦀\n";
    let plan = Plan::prepare(&config, &options, input.into()).unwrap();
    assert!(!plan.needs_slack());
    let mut output = Vec::new();
    let outcomes = plan.execute(None, &mut output).unwrap();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(output, input.as_bytes());
    assert_eq!(exit_code(&outcomes), 0);
}

#[test]
fn slack_unicode_limit_and_plain_text_payload_do_not_change_stdout() {
    let (_directory, config) = fixture();
    let mut options = args(&["slack:C123"]);
    options.passthrough = true;
    options.max_chars = Some(4);
    let original = "🦀<&>rest\n";
    let plan = Plan::prepare(&config, &options, original.into()).unwrap();
    let (slack, server) = server(vec![(200, json!({"ok": true}), None)]);
    let mut output = Vec::new();
    let outcomes = plan.execute(Some(&slack), &mut output).unwrap();
    assert_eq!(output, original.as_bytes());
    assert_eq!(exit_code(&outcomes), 0);
    let requests = server.join().unwrap();
    assert_eq!(requests[0].path, "/chat.postMessage");
    assert!(requests[0].authorized);
    assert_eq!(requests[0].body["text"], "🦀&lt;&amp;&gt;");
    assert_eq!(requests[0].body["mrkdwn"], false);
    assert_eq!(requests[0].body["unfurl_links"], false);
}

#[test]
fn argument_newline_is_only_added_to_stdout() {
    let (_directory, config) = fixture();
    let mut options = args(&["slack:C123", "stdout", "stdout"]);
    options.message = vec!["hello".into(), "world".into()];
    options.passthrough = true;
    let plan = Plan::prepare(&config, &options, "hello world".into()).unwrap();
    let (slack, server) = server(vec![(200, json!({"ok": true}), None)]);
    let mut output = Vec::new();
    let outcomes = plan.execute(Some(&slack), &mut output).unwrap();
    assert_eq!(exit_code(&outcomes), 0);
    assert_eq!(outcomes.len(), 2);
    assert_eq!(output, b"hello world\n");
    assert_eq!(server.join().unwrap()[0].body["text"], "hello world");
}

#[test]
fn partial_failure_does_not_stop_other_destinations_or_retry() {
    let (_directory, config) = fixture();
    let plan = Plan::prepare(
        &config,
        &args(&["slack:C123", "stdout", "slack:U123"]),
        "hello".into(),
    )
    .unwrap();
    let (slack, server) = server(vec![
        (200, json!({"ok": false, "error": "invalid_auth"}), None),
        (200, json!({"ok": true, "channel": {"id": "D123"}}), None),
        (200, json!({"ok": true}), None),
    ]);
    let mut output = Vec::new();
    let outcomes = plan.execute(Some(&slack), &mut output).unwrap();
    assert_eq!(exit_code(&outcomes), 4);
    assert_eq!(outcomes[0].status, Status::Failed);
    assert_eq!(outcomes[2].status, Status::Accepted);
    assert_eq!(output, b"hello");
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[1].path, "/conversations.open");
    assert_eq!(requests[1].body["users"], "U123");
    assert_eq!(requests[2].body["channel"], "D123");
}

#[test]
fn rate_limit_unknown_response_and_server_failure_are_safe() {
    for (code, response, retry, status, expected) in [
        (429, json!({}), Some(12), Status::Failed, "12 seconds"),
        (
            503,
            json!({}),
            None,
            Status::Uncertain,
            "acceptance is unknown",
        ),
        (
            200,
            json!({"error": "fake-test-token"}),
            None,
            Status::Uncertain,
            "no acceptance status",
        ),
        (
            200,
            json!({"ok": false, "error": "fake-test-token"}),
            None,
            Status::Failed,
            "rejected",
        ),
        (302, json!({}), None, Status::Failed, "HTTP 302"),
    ] {
        let (slack, server) = server(vec![(code, response, retry)]);
        let failure = slack
            .send("fake-test-token", Some("C123"), None, "hi")
            .unwrap_err();
        assert_eq!(failure.status, status);
        assert!(failure.message.contains(expected));
        assert!(!failure.message.contains("fake-test-token"));
        assert_eq!(server.join().unwrap().len(), 1);
    }
}

#[test]
fn preflight_rejects_bad_destinations_missing_secrets_and_oversize() {
    let (directory, config) = fixture();
    assert!(Plan::prepare(&config, &args(&["stdout", "missing"]), "hi".into()).is_err());
    assert!(Plan::prepare(&config, &args(&["slack:C123"]), "&".repeat(10_000)).is_err());
    let mut options = args(&["stdout"]);
    options.max_chars = Some(0);
    assert!(Plan::prepare(&config, &options, "hi".into()).is_err());
    fs::remove_file(directory.path().join("token")).unwrap();
    assert!(Plan::prepare(&config, &args(&["stdout", "slack:C123"]), "hi".into()).is_err());
}

#[test]
fn credential_permissions_and_symlinks_are_checked() {
    let (directory, _) = fixture();
    let path = directory.path().join("token");
    let reference = SecretRef::File { path: path.clone() };
    assert_eq!(
        secrets::resolve(&reference).unwrap().as_str(),
        "fake-test-token"
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(secrets::resolve(&reference).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let link = directory.path().join("link");
    symlink(&path, &link).unwrap();
    assert!(secrets::resolve(&SecretRef::File { path: link }).is_err());
}

#[test]
fn edits_preserve_comments_support_inline_tables_and_reject_plaintext_tokens() {
    let mut doc = r#"# keep this comment
timeout_seconds = 30
[accounts.work]
type = "slack"
token_secret = { source = "env", name = "OLD_TOKEN" }
"#
    .parse()
    .unwrap();
    config::modify(
        &mut doc,
        "accounts.work.token_secret.name",
        Some("NEW_TOKEN"),
    )
    .unwrap();
    assert!(doc.to_string().contains("# keep this comment"));
    assert_eq!(
        config::get(
            &config::decode(&doc).unwrap(),
            "accounts.work.token_secret.name"
        )
        .unwrap(),
        "NEW_TOKEN"
    );
    assert!(config::modify(&mut doc, "accounts.work.token", Some("secret-value")).is_err());
    assert!(config::decode(&"unknown = true".parse().unwrap()).is_err());
    assert!(config::decode(&"default_outputs = []".parse().unwrap()).is_err());
}

#[test]
fn broken_stdout_does_not_prevent_slack_delivery() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let (_directory, config) = fixture();
    let plan = Plan::prepare(&config, &args(&["stdout", "slack:C123"]), "hi".into()).unwrap();
    let (slack, server) = server(vec![(200, json!({"ok": true}), None)]);
    let outcomes = plan.execute(Some(&slack), &mut Broken).unwrap();
    assert_eq!(exit_code(&outcomes), 4);
    assert_eq!(outcomes[0].status, Status::Failed);
    assert_eq!(outcomes[1].status, Status::Accepted);
    server.join().unwrap();
}

#[test]
fn prefixed_targets_resolve_all_names_before_any_delivery() {
    let (_directory, config) = fixture();
    let plan = Plan::prepare(
        &config,
        &args(&["stdout", "slack:C123", "slack:#ops", "slack:@dustin"]),
        "hello".into(),
    )
    .unwrap();
    let (slack, server) = server(vec![
        (
            200,
            json!({"ok": true, "channels": [], "response_metadata": {"next_cursor": "next-page="}}),
            None,
        ),
        (
            200,
            json!({"ok": true, "channels": [{"name": "ops", "id": "C456"}]}),
            None,
        ),
        (
            200,
            json!({"ok": true, "members": [{"name": "someone-else", "id": "U999", "profile": {"display_name": "dustin"}}], "response_metadata": {"next_cursor": "more-users"}}),
            None,
        ),
        (
            200,
            json!({"ok": true, "members": [{"name": "dustin", "id": "U456", "deleted": false}]}),
            None,
        ),
        (200, json!({"ok": true}), None),
        (200, json!({"ok": true}), None),
        (200, json!({"ok": true, "channel": {"id": "D456"}}), None),
        (200, json!({"ok": true}), None),
    ]);
    let mut output = Vec::new();
    let outcomes = plan.execute(Some(&slack), &mut output).unwrap();
    assert_eq!(exit_code(&outcomes), 0);
    assert_eq!(output, b"hello");
    assert_eq!(outcomes[2].destination, "slack:#ops");
    assert_eq!(outcomes[3].destination, "slack:@dustin");
    let requests = server.join().unwrap();
    assert_eq!(
        requests.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(),
        [
            "/conversations.list",
            "/conversations.list",
            "/users.list",
            "/users.list",
            "/chat.postMessage",
            "/chat.postMessage",
            "/conversations.open",
            "/chat.postMessage",
        ]
    );
    assert!(requests.iter().all(|r| r.authorized));
    assert_eq!(requests[0].body["types"], "public_channel,private_channel");
    assert_eq!(requests[1].body["cursor"], "next-page=");
    assert_eq!(requests[3].body["cursor"], "more-users");
    assert_eq!(requests[4].body["channel"], "C123");
    assert_eq!(requests[5].body["channel"], "C456");
    assert_eq!(requests[6].body["users"], "U456");
    assert_eq!(requests[7].body["channel"], "D456");
}

#[test]
fn lookup_errors_send_nothing_and_do_not_match_display_names_or_deleted_users() {
    for response in [
        json!({"ok": false, "error": "missing_scope"}),
        json!({"ok": true, "members": [{"name": "other", "id": "U1", "profile": {"display_name": "dustin"}}]}),
        json!({"ok": true, "members": [{"name": "dustin", "id": "U1", "deleted": true}]}),
        json!({"ok": true, "members": [{"name": "dustin", "id": "U1"}, {"name": "dustin", "id": "U2"}]}),
        json!({"ok": true, "members": [{"name": "dustin", "id": "not-a-slack-id"}]}),
    ] {
        let (_directory, config) = fixture();
        let plan = Plan::prepare(
            &config,
            &args(&["stdout", "slack:C123", "slack:@dustin"]),
            "hello".into(),
        )
        .unwrap();
        let (slack, server) = server(vec![(200, response, None)]);
        let mut output = Vec::new();
        let error = plan.execute(Some(&slack), &mut output).unwrap_err();
        assert!(format!("{error:#}").contains("no messages sent"));
        assert!(output.is_empty());
        assert_eq!(server.join().unwrap().len(), 1);
    }
}

#[test]
fn repeated_cursors_and_lookup_rate_limits_fail_safely() {
    let page = json!({"ok": true, "channels": [], "response_metadata": {"next_cursor": "same"}});
    let (slack, server) = server(vec![(200, page.clone(), None), (200, page, None)]);
    let error = slack.lookup("fake-test-token", "ops", false).unwrap_err();
    assert!(error.to_string().contains("repeated a pagination cursor"));
    assert_eq!(server.join().unwrap().len(), 2);
    let (slack, server) = self::server(vec![(429, json!({}), Some(9))]);
    let error = slack.lookup("fake-test-token", "dustin", true).unwrap_err();
    assert!(error.to_string().contains("9 seconds"));
    assert!(error.to_string().contains("users:read"));
    server.join().unwrap();
}

#[test]
fn account_selection_is_explicit_when_multiple_accounts_exist() {
    let (_directory, mut config) = fixture();
    assert!(Plan::prepare(&config, &args(&["slack:C123"]), "hi".into()).is_ok());
    config.accounts.insert(
        "other".into(),
        config::Account {
            kind: config::AccountType::Slack,
            token_secret: SecretRef::File {
                path: "/nonexistent-msg-test-secret".into(),
            },
        },
    );
    assert!(
        Plan::prepare(&config, &args(&["slack:C123"]), "hi".into())
            .err()
            .unwrap()
            .to_string()
            .contains("multiple Slack accounts")
    );
    config.default_slack_account = Some("work".into());
    assert!(Plan::prepare(&config, &args(&["slack:C123"]), "hi".into()).is_ok());
    config.default_slack_account = Some("other".into());
    let mut options = args(&["slack:C123"]);
    options.account = Some("work".into());
    assert!(Plan::prepare(&config, &options, "hi".into()).is_ok());
    options.account = Some("missing".into());
    assert!(Plan::prepare(&config, &options, "hi".into()).is_err());
    options.account = Some("other".into());
    assert!(Plan::prepare(&config, &options, "hi".into()).is_err());
}

#[test]
fn service_prefixed_ids_and_duplicate_arguments_need_no_directory_lookup() {
    let (_directory, config) = fixture();
    let plan = Plan::prepare(
        &config,
        &args(&["slack:C123", "stdout", "slack:U123", "slack:C123"]),
        "hi".into(),
    )
    .unwrap();
    let (slack, server) = server(vec![
        (200, json!({"ok": true}), None),
        (200, json!({"ok": true, "channel": {"id": "D123"}}), None),
        (200, json!({"ok": true}), None),
    ]);
    let mut output = Vec::new();
    let outcomes = plan.execute(Some(&slack), &mut output).unwrap();
    assert_eq!(exit_code(&outcomes), 0);
    assert_eq!(outcomes.len(), 3);
    assert_eq!(output, b"hi");
    assert_eq!(
        server
            .join()
            .unwrap()
            .iter()
            .map(|r| r.path.as_str())
            .collect::<Vec<_>>(),
        [
            "/chat.postMessage",
            "/conversations.open",
            "/chat.postMessage"
        ]
    );
}

#[test]
fn target_syntax_rejects_bad_prefixes_and_empty_recipients() {
    for text in [
        "slack:",
        "slack:#",
        "slack:@",
        "slack:bare-name",
        "team",
        "email:user",
        "slack:@a b",
        "slack:#bad\nname",
    ] {
        assert!(target::parse(text).is_err());
    }
    for text in [
        "stdout",
        "slack:#my-channel",
        "slack:@dustin.name",
        "slack:C123",
        "slack:U123",
    ] {
        assert!(target::parse(text).is_ok());
    }
}
