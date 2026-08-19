use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use mcp_doctor::{
    CheckContext, FileReport, Finding, Severity, annotate_server_name_conflicts, discover_paths,
    inspect_discovered_file_for_workspace, inspect_file_for_workspace,
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Human,
    Json,
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
    let mut files = Vec::new();
    let mut errors = Vec::new();

    for path in paths {
        let inspection = if explicit {
            inspect_file_for_workspace(&path, &context, &workspace).map(Some)
        } else {
            inspect_discovered_file_for_workspace(&path, &context, &workspace)
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
    if cli.ci && output.summary.errors > 0 {
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
        }
        for finding in &file.findings {
            print_finding(finding);
        }
    }
    for error in &output.errors {
        eprintln!("Input error: {}", terminal_text(&error.message));
    }
    println!(
        "\nSummary: {} file(s), {} server(s), {} finding(s), {} error(s), {} warning(s)",
        output.summary.files,
        output.summary.servers,
        output.summary.findings,
        output.summary.errors,
        output.summary.warnings
    );
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
