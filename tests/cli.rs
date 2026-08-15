use std::fs;

use assert_cmd::Command;
use tempfile::tempdir;

fn command() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("mcp-doctor"))
}

#[test]
fn human_output_reports_findings_without_failing_by_default() {
    let dir = tempdir().expect("tempdir");
    let config = dir.path().join("mcp.json");
    fs::write(
        &config,
        r#"{"mcpServers":{"demo":{"command":"missing-mcp-command","env":{"TOKEN":"never-print-this"}}}}"#,
    )
    .expect("write config");

    let output = command()
        .arg("--no-discover")
        .arg(&config)
        .output()
        .expect("run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("ERROR"));
    assert!(stdout.contains("command_not_found"));
    assert!(!stdout.contains("never-print-this"));
}

#[test]
fn ci_mode_exits_one_for_check_errors() {
    let dir = tempdir().expect("tempdir");
    let config = dir.path().join("mcp.json");
    fs::write(
        &config,
        r#"{"mcpServers":{"demo":{"command":"missing-mcp-command"}}}"#,
    )
    .expect("write config");

    let output = command()
        .arg("--ci")
        .arg("--no-discover")
        .arg(&config)
        .output()
        .expect("run");

    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn json_output_is_structured_and_redacted() {
    let dir = tempdir().expect("tempdir");
    let config = dir.path().join("mcp.json");
    fs::write(
        &config,
        r#"{"mcpServers":{"demo":{"command":"missing-mcp-command","env":{"TOKEN":"never-print-this"}}}}"#,
    )
    .expect("write config");

    let output = command()
        .arg("--format")
        .arg("json")
        .arg("--no-discover")
        .arg(&config)
        .output()
        .expect("run");

    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert_eq!(value["summary"]["errors"], 1);
    assert_eq!(value["files"][0]["servers"][0]["name"], "demo");
    assert!(
        !String::from_utf8(output.stdout)
            .expect("utf8 stdout")
            .contains("never-print-this")
    );
}

#[test]
fn cli_inspects_explicit_devcontainer_mcp_configuration() {
    let dir = tempdir().expect("tempdir");
    let config = dir.path().join("devcontainer.json");
    fs::write(
        &config,
        r#"{"customizations":{"vscode":{"mcp":{"servers":{"demo":{"command":"missing-mcp-command"}}}}}}"#,
    )
    .expect("write config");

    let output = command()
        .arg("--no-discover")
        .arg(&config)
        .output()
        .expect("run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("Server: demo"));
    assert!(stdout.contains("command_not_found"));
}

#[test]
fn discovery_inspects_claude_code_user_and_current_workspace_scopes() {
    let home = tempdir().expect("home");
    let workspace = tempdir().expect("workspace");
    let app_data = tempdir().expect("app data");
    let config = home.path().join(".claude.json");
    fs::write(
        &config,
        serde_json::to_vec(&serde_json::json!({
            "mcpServers": {
                "user-server": {"command": "missing-user-command"}
            },
            "projects": {
                workspace.path().to_string_lossy(): {
                    "mcpServers": {
                        "local-server": {"command": "missing-local-command"}
                    }
                },
                "/another/project": {
                    "mcpServers": {
                        "other-server": {
                            "command": "missing-other-command",
                            "env": {"TOKEN": "never-print-this"}
                        }
                    }
                }
            }
        }))
        .expect("serialize config"),
    )
    .expect("write config");

    let output = command()
        .current_dir(workspace.path())
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("APPDATA", app_data.path())
        .output()
        .expect("run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("Server: user-server"));
    assert!(stdout.contains("Server: local-server"));
    assert!(!stdout.contains("other-server"));
    assert!(!stdout.contains("never-print-this"));
}

#[test]
fn discovery_inspects_codex_user_and_project_configs_and_reports_conflicts() {
    let home = tempdir().expect("home");
    let workspace = tempdir().expect("workspace");
    let app_data = tempdir().expect("app data");
    let user_config = home.path().join(".codex/config.toml");
    let project_config = workspace.path().join(".codex/config.toml");
    fs::create_dir_all(user_config.parent().expect("user config parent"))
        .expect("create user config parent");
    fs::create_dir_all(project_config.parent().expect("project config parent"))
        .expect("create project config parent");
    fs::write(
        &user_config,
        r#"
            [mcp_servers.Playwright]
            command = "missing-user-command"
            env = { TOKEN = "never-print-this" }
        "#,
    )
    .expect("write user config");
    fs::write(
        &project_config,
        r#"
            [mcp_servers.playwright]
            command = "missing-project-command"
        "#,
    )
    .expect("write project config");

    let output = command()
        .current_dir(workspace.path())
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("APPDATA", app_data.path())
        .output()
        .expect("run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert_eq!(stdout.matches("server_name_conflict").count(), 2);
    assert!(stdout.contains("Server: Playwright"));
    assert!(stdout.contains("Server: playwright"));
    assert!(!stdout.contains("never-print-this"));
}

#[test]
fn reports_conflicting_server_names_across_explicit_files() {
    let dir = tempdir().expect("tempdir");
    let user_config = dir.path().join("user-mcp.json");
    let repo_config = dir.path().join("repo-mcp.json");
    fs::write(
        &user_config,
        r#"{"mcpServers":{"MCPBrowser":{"command":"missing-user-command"}}}"#,
    )
    .expect("write user config");
    fs::write(
        &repo_config,
        r#"{"mcpServers":{"mcpbrowser":{"command":"missing-repo-command"}}}"#,
    )
    .expect("write repo config");

    let output = command()
        .arg("--no-discover")
        .arg(&user_config)
        .arg(&repo_config)
        .output()
        .expect("run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert_eq!(stdout.matches("server_name_conflict").count(), 2);
    assert!(stdout.contains("multiple inspected configuration entries"));
}

#[test]
fn json_output_exposes_server_name_conflicts_as_warnings() {
    let dir = tempdir().expect("tempdir");
    let user_config = dir.path().join("user-mcp.json");
    let repo_config = dir.path().join("repo-mcp.json");
    fs::write(
        &user_config,
        r#"{"mcpServers":{"playwright":{"command":"missing-user-command"}}}"#,
    )
    .expect("write user config");
    fs::write(
        &repo_config,
        r#"{"mcpServers":{"Playwright":{"command":"missing-repo-command"}}}"#,
    )
    .expect("write repo config");

    let output = command()
        .arg("--format")
        .arg("json")
        .arg("--no-discover")
        .arg(&user_config)
        .arg(&repo_config)
        .output()
        .expect("run");

    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert_eq!(value["summary"]["warnings"], 2);
    assert!(
        value["files"]
            .as_array()
            .expect("files")
            .iter()
            .all(|file| {
                file["findings"]
                    .as_array()
                    .expect("findings")
                    .iter()
                    .any(|finding| {
                        finding["code"] == "server_name_conflict"
                            && finding["severity"] == "warning"
                    })
            })
    );
}

#[test]
fn explicit_input_error_exits_two() {
    let dir = tempdir().expect("tempdir");
    let missing = dir.path().join("missing.json");

    let output = command()
        .arg("--no-discover")
        .arg(missing)
        .output()
        .expect("run");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .expect("utf8 stderr")
            .contains("cannot read")
    );
}

#[test]
fn no_discovery_with_no_inputs_is_an_empty_success() {
    let output = command().arg("--no-discover").output().expect("run");

    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .expect("utf8 stdout")
            .contains("No MCP configuration files found")
    );
}

#[test]
fn human_output_escapes_terminal_control_sequences() {
    let dir = tempdir().expect("tempdir");
    let config = dir.path().join("mcp.json");
    fs::write(
        &config,
        "{\"mcpServers\":{\"\\u001b]8;;https://example.test\\u0007中文\":{\"command\":\"missing\"}}}",
    )
    .expect("write config");

    let output = command()
        .arg("--no-discover")
        .arg(&config)
        .output()
        .expect("run");

    assert!(output.status.success());
    assert!(!output.stdout.contains(&0x1b));
    assert!(!output.stdout.contains(&0x07));
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("\\u{1b}"));
    assert!(stdout.contains("中文"));
}

#[cfg(target_os = "linux")]
#[test]
fn json_output_serializes_non_utf8_paths() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let dir = tempdir().expect("tempdir");
    let config = dir
        .path()
        .join(OsString::from_vec(b"mcp-\xff.json".to_vec()));
    fs::write(&config, r#"{"mcpServers":{"demo":{"command":"missing"}}}"#).expect("write config");

    let output = command()
        .arg("--format")
        .arg("json")
        .arg("--no-discover")
        .arg(&config)
        .output()
        .expect("run");

    assert!(output.status.success());
    serde_json::from_slice::<serde_json::Value>(&output.stdout).expect("valid JSON");
}
