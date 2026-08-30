use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use mcp_doctor::{
    CheckContext, FileReport, Finding, PortabilityStatus, PortabilityTarget, Severity,
    annotate_server_name_conflicts, discover_paths, inspect_discovered_file_for_workspace,
    inspect_discovered_file_for_workspace_and_target, inspect_file_for_workspace,
    inspect_file_for_workspace_and_target,
};
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(
    name = "mcp-doctor",
    version,
    about = "Static preflight checks for local stdio MCP configurations"
)]
struct Cli {
    /// Configuration files to inspect. Without paths, known local paths are discovered.
    #[arg(value_name = "CONFIG")]
    configs: Vec<PathBuf>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,

    /// Return exit code 1 when a check error is found.
    #[arg(long)]
    ci: bool,

    /// Do not inspect discovered paths when no CONFIG is supplied.
    #[arg(long)]
    no_discover: bool,

    /// Report whether server semantics are portable to another MCP client.
    #[arg(long, value_enum)]
    portability_target: Option<PortabilityTargetArg>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Human,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum PortabilityTargetArg {
    Codex,
    #[value(name = "claude-code")]
    ClaudeCode,
    #[value(name = "vscode")]
    VsCode,
    Cursor,
}

impl From<PortabilityTargetArg> for PortabilityTarget {
    fn from(value: PortabilityTargetArg) -> Self {
        match value {
            PortabilityTargetArg::Codex => Self::Codex,
            PortabilityTargetArg::ClaudeCode => Self::ClaudeCode,
            PortabilityTargetArg::VsCode => Self::VsCode,
            PortabilityTargetArg::Cursor => Self::Cursor,
        }
    }
}

#[derive(Debug, Serialize)]
struct InputError {
    path: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct Summary {
    files: usize,
    servers: usize,
    findings: usize,
    errors: usize,
    warnings: usize,
    portability_issues: usize,
}

#[derive(Debug, Serialize)]
struct Output {
    files: Vec<FileReport>,
    errors: Vec<InputError>,
    summary: Summary,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let workspace = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let explicit = !cli.configs.is_empty();
    let paths = if explicit || cli.no_discover {
        cli.configs
    } else {
        discover_paths(&workspace)
    };
    let context = CheckContext::from_system();
    let portability_target = cli.portability_target.map(PortabilityTarget::from);
    let mut files = Vec::new();
    let mut errors = Vec::new();

    for path in paths {
        let inspection = if explicit {
            match portability_target {
                Some(target) => {
                    inspect_file_for_workspace_and_target(&path, &context, &workspace, target)
                }
                None => inspect_file_for_workspace(&path, &context, &workspace),
            }
            .map(Some)
        } else {
            match portability_target {
                Some(target) => inspect_discovered_file_for_workspace_and_target(
                    &path, &context, &workspace, target,
                ),
                None => inspect_discovered_file_for_workspace(&path, &context, &workspace),
            }
        };
        match inspection {
            Ok(Some(report)) => files.push(report),
            Ok(None) => {}
            Err(error) => errors.push(InputError {
                path: path.to_string_lossy().into_owned(),
                message: error.to_string(),
            }),
        }
    }
    annotate_server_name_conflicts(&mut files);
    let output = build_output(files, errors);
    match cli.format {
        OutputFormat::Human => print_human(&output),
        OutputFormat::Json => {
            if !print_json(&output) {
                return ExitCode::from(2);
            }
        }
    }

    if !output.errors.is_empty() {
        return ExitCode::from(2);
    }
    if cli.ci && (output.summary.errors > 0 || output.summary.portability_issues > 0) {
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn build_output(files: Vec<FileReport>, errors: Vec<InputError>) -> Output {
    let summary = Summary {
        files: files.len(),
        servers: files.iter().map(|file| file.servers.len()).sum(),
        findings: files.iter().map(|file| file.findings.len()).sum(),
        errors: files
            .iter()
            .flat_map(|file| file.findings.iter())
            .filter(|finding| finding.severity == Severity::Error)
            .count(),
        warnings: files
            .iter()
            .flat_map(|file| file.findings.iter())
            .filter(|finding| finding.severity == Severity::Warning)
            .count(),
        portability_issues: files
            .iter()
            .flat_map(|file| file.servers.iter())
            .filter_map(|server| server.portability.as_ref())
            .filter(|assessment| assessment.status != PortabilityStatus::Portable)
            .count(),
    };
    Output {
        files,
        errors,
        summary,
    }
}

fn print_human(output: &Output) {
    if output.files.is_empty() && output.errors.is_empty() {
        println!("No MCP configuration files found.");
        return;
    }
    println!("MCP Doctor (static stdio preflight)");
    for file in &output.files {
        println!("\nConfig: {}", terminal_text(&file.path.to_string_lossy()));
        for server in &file.servers {
            println!(
                "  Server: {} [{}]",
                terminal_text(&server.name),
                terminal_text(server.transport)
            );
            if let Some(portability) = &server.portability {
                println!(
                    "    Portability to {}: {}",
                    json_name(&portability.target),
                    json_name(&portability.status)
                );
                for reason in &portability.reasons {
                    println!(
                        "      {} [{}]: {}",
                        serde_json::to_string(&reason.code)
                            .unwrap_or_else(|_| "\"unknown\"".to_string())
                            .trim_matches('"'),
                        reason.location,
                        reason.message
                    );
                }
            }
        }
        for finding in &file.findings {
            print_finding(finding);
        }
    }
    for error in &output.errors {
        eprintln!("Input error: {}", terminal_text(&error.message));
    }
    println!(
        "\nSummary: {} file(s), {} server(s), {} finding(s), {} error(s), {} warning(s), {} portability issue(s)",
        output.summary.files,
        output.summary.servers,
        output.summary.findings,
        output.summary.errors,
        output.summary.warnings,
        output.summary.portability_issues
    );
}

fn json_name(value: &impl Serialize) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|_| "\"unknown\"".to_string())
        .trim_matches('"')
        .to_string()
}

fn print_finding(finding: &Finding) {
    let severity = match finding.severity {
        Severity::Error => "ERROR",
        Severity::Warning => "WARN",
        Severity::Info => "INFO",
    };
    let server = finding.server.as_deref().unwrap_or("config");
    println!(
        "  {severity} {code} [{server}::{location}]: {message}",
        code = serde_json::to_string(&finding.code)
            .unwrap_or_else(|_| "\"unknown\"".to_string())
            .trim_matches('"'),
        server = terminal_text(server),
        location = terminal_text(&finding.location),
        message = terminal_text(&finding.message)
    );
}

fn print_json(output: &Output) -> bool {
    match serde_json::to_string_pretty(output) {
        Ok(value) => {
            println!("{value}");
            true
        }
        Err(error) => {
            eprintln!("cannot serialize report: {error}");
            false
        }
    }
}

fn terminal_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_control() {
            escaped.extend(character.escape_default());
        } else {
            escaped.push(character);
        }
    }
    escaped
}
