use std::fs;

use assert_cmd::Command;
use serde_json::{Value, json};
use tempfile::tempdir;

fn command() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("mcp-doctor"))
}

fn write_transcript(path: &std::path::Path, envelopes: &[Value]) {
    let mut contents = envelopes
        .iter()
        .map(|envelope| serde_json::to_string(envelope).expect("serialize envelope"))
        .collect::<Vec<_>>()
        .join("\n");
    contents.push('\n');
    fs::write(path, contents).expect("write transcript");
}

#[test]
fn valid_stdio_initialize_handshake_has_no_findings() {
    let dir = tempdir().expect("tempdir");
    let transcript = dir.path().join("valid_initialize.jsonl");
    write_transcript(
        &transcript,
        &[
            json!({
                "schema_version": 1,
                "direction": "client_to_server",
                "payload": r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test-client","version":"1.0.0"}}}"#
            }),
            json!({
                "schema_version": 1,
                "direction": "server_to_client",
                "payload": r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"test-server","version":"1.0.0"}}}"#
            }),
            json!({
                "schema_version": 1,
                "direction": "client_to_server",
                "payload": r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
            }),
        ],
    );

    let output = command()
        .arg("transcript")
        .arg(&transcript)
        .output()
        .expect("run transcript lint");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("0 finding(s)"), "stdout was: {stdout}");
}

#[test]
fn server_stdout_pollution_is_redacted_and_only_fails_in_ci_mode() {
    const SENTINEL: &str = "never-print-transcript-stdout-payload";

    let dir = tempdir().expect("tempdir");
    let transcript = dir.path().join("stdout_pollution.jsonl");
    write_transcript(
        &transcript,
        &[json!({
            "schema_version": 1,
            "direction": "server_to_client",
            "payload": SENTINEL
        })],
    );

    let default_output = command()
        .arg("transcript")
        .arg(&transcript)
        .output()
        .expect("run transcript lint");
    assert!(default_output.status.success());
    let default_stdout = String::from_utf8(default_output.stdout).expect("utf8 stdout");
    let default_stderr = String::from_utf8(default_output.stderr).expect("utf8 stderr");
    assert!(default_stdout.contains("transcript_stdout_pollution"));
    assert!(!default_stdout.contains(SENTINEL));
    assert!(!default_stderr.contains(SENTINEL));

    let ci_output = command()
        .arg("--ci")
        .arg("transcript")
        .arg(&transcript)
        .output()
        .expect("run transcript lint in CI mode");
    assert_eq!(ci_output.status.code(), Some(1));
    let ci_stdout = String::from_utf8(ci_output.stdout).expect("utf8 stdout");
    let ci_stderr = String::from_utf8(ci_output.stderr).expect("utf8 stderr");
    assert!(ci_stdout.contains("transcript_stdout_pollution"));
    assert!(!ci_stdout.contains(SENTINEL));
    assert!(!ci_stderr.contains(SENTINEL));
}

#[test]
fn embedded_newline_payload_is_reported_without_echoing_payload() {
    const SENTINEL: &str = "never-print-multiline-transcript-payload";

    let dir = tempdir().expect("tempdir");
    let transcript = dir.path().join("multiline_message.jsonl");
    write_transcript(
        &transcript,
        &[json!({
            "schema_version": 1,
            "direction": "server_to_client",
            "payload": format!(
                "{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/message\"}}\n{SENTINEL}"
            )
        })],
    );

    let output = command()
        .arg("transcript")
        .arg(&transcript)
        .output()
        .expect("run transcript lint");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stdout.contains("transcript_multiline_message"));
    assert!(!stdout.contains(SENTINEL));
    assert!(!stderr.contains(SENTINEL));
}

#[test]
fn jsonrpc_object_without_request_response_or_notification_shape_is_rejected() {
    let dir = tempdir().expect("tempdir");
    let transcript = dir.path().join("invalid_shape.jsonl");
    write_transcript(
        &transcript,
        &[json!({
            "schema_version": 1,
            "direction": "server_to_client",
            "payload": r#"{"jsonrpc":"2.0"}"#
        })],
    );

    let output = command()
        .arg("transcript")
        .arg(&transcript)
        .output()
        .expect("run transcript lint");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("transcript_invalid_jsonrpc_message"));
}

#[test]
fn initialized_before_initialize_response_is_reported() {
    let dir = tempdir().expect("tempdir");
    let transcript = dir.path().join("out_of_order.jsonl");
    write_transcript(
        &transcript,
        &[
            json!({
                "schema_version": 1,
                "direction": "client_to_server",
                "payload": r#"{"jsonrpc":"2.0","id":"init","method":"initialize","params":{}}"#
            }),
            json!({
                "schema_version": 1,
                "direction": "client_to_server",
                "payload": r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
            }),
        ],
    );

    let output = command()
        .arg("--format")
        .arg("json")
        .arg("transcript")
        .arg(&transcript)
        .output()
        .expect("run transcript lint");

    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).expect("valid JSON report");
    assert_eq!(report["kind"], "mcp_transcript");
    assert_eq!(report["summary"]["errors"], 1);
    assert_eq!(
        report["transcripts"][0]["findings"][0]["code"],
        "transcript_initialize_order"
    );
}

#[test]
fn duplicate_initialize_request_is_reported_once() {
    let dir = tempdir().expect("tempdir");
    let transcript = dir.path().join("duplicate_initialize.jsonl");
    write_transcript(
        &transcript,
        &[
            json!({
                "schema_version": 1,
                "direction": "client_to_server",
                "payload": r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#
            }),
            json!({
                "schema_version": 1,
                "direction": "client_to_server",
                "payload": r#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":{}}"#
            }),
        ],
    );

    let output = command()
        .arg("transcript")
        .arg(&transcript)
        .output()
        .expect("run transcript lint");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert_eq!(stdout.matches("transcript_duplicate_initialize").count(), 1);
}

#[test]
fn invalid_outer_record_is_an_input_error_without_payload_echo() {
    const SENTINEL: &str = "never-print-invalid-outer-record";
    let dir = tempdir().expect("tempdir");
    let transcript = dir.path().join("invalid_outer.jsonl");
    fs::write(&transcript, format!("not-json-{SENTINEL}\n")).expect("write transcript");

    let output = command()
        .arg("--format")
        .arg("json")
        .arg("transcript")
        .arg(&transcript)
        .output()
        .expect("run transcript lint");

    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    let report: Value = serde_json::from_str(&stdout).expect("valid JSON report");
    assert_eq!(report["errors"].as_array().expect("errors").len(), 1);
    assert!(!stdout.contains(SENTINEL));
    assert!(!stderr.contains(SENTINEL));
}

#[test]
fn invalid_request_id_type_is_rejected() {
    let dir = tempdir().expect("tempdir");
    let transcript = dir.path().join("invalid_id.jsonl");
    write_transcript(
        &transcript,
        &[json!({
            "schema_version": 1,
            "direction": "client_to_server",
            "payload": r#"{"jsonrpc":"2.0","id":{"secret":"never-print-id"},"method":"initialize","params":{}}"#
        })],
    );

    let output = command()
        .arg("transcript")
        .arg(&transcript)
        .output()
        .expect("run transcript lint");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("transcript_invalid_jsonrpc_message"));
    assert!(!stdout.contains("never-print-id"));
}

#[test]
fn oversized_payload_is_a_redacted_input_error() {
    let dir = tempdir().expect("tempdir");
    let transcript = dir.path().join("oversized.jsonl");
    write_transcript(
        &transcript,
        &[json!({
            "schema_version": 1,
            "direction": "server_to_client",
            "payload": "x".repeat(1024 * 1024 + 1)
        })],
    );

    let output = command()
        .arg("--format")
        .arg("json")
        .arg("transcript")
        .arg(&transcript)
        .output()
        .expect("run transcript lint");

    assert_eq!(output.status.code(), Some(2));
    let report: Value = serde_json::from_slice(&output.stdout).expect("valid JSON report");
    assert!(
        report["errors"][0]["message"]
            .as_str()
            .expect("message")
            .contains("payload exceeds 1 MiB")
    );
    assert!(output.stdout.len() < 4096);
}

#[test]
fn finding_saturation_does_not_bypass_record_count_limit() {
    let dir = tempdir().expect("tempdir");
    let transcript = dir.path().join("too_many_records.jsonl");
    let envelope = serde_json::to_string(&json!({
        "schema_version": 1,
        "direction": "server_to_client",
        "payload": "not-json"
    }))
    .expect("serialize envelope");
    let mut contents = String::with_capacity((envelope.len() + 1) * 100_001);
    for _ in 0..100_001 {
        contents.push_str(&envelope);
        contents.push('\n');
    }
    fs::write(&transcript, contents).expect("write transcript");

    let output = command()
        .arg("--format")
        .arg("json")
        .arg("transcript")
        .arg(&transcript)
        .output()
        .expect("run transcript lint");

    assert_eq!(output.status.code(), Some(2));
    let report: Value = serde_json::from_slice(&output.stdout).expect("valid JSON report");
    assert!(
        report["errors"][0]["message"]
            .as_str()
            .expect("message")
            .contains("100000 records")
    );
}
