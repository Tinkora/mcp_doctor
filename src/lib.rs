//! Library API for static MCP stdio configuration diagnostics.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Serialize, Serializer};
use serde_json::{Map, Value};
use thiserror::Error;

/// A restricted environment view used to resolve command search paths and
/// test environment-variable presence. Environment values are deliberately
/// not part of this context.
#[derive(Clone, Debug, Default)]
pub struct CheckContext {
    pub path_entries: Vec<PathBuf>,
    pub command_extensions: Vec<String>,
    pub environment_names: BTreeSet<String>,
}

impl CheckContext {
    pub fn with_path<I>(entries: I) -> Self
    where
        I: IntoIterator<Item = PathBuf>,
    {
        Self {
            path_entries: entries.into_iter().collect(),
            command_extensions: default_command_extensions(),
            environment_names: BTreeSet::new(),
        }
    }

    pub fn with_path_and_extensions<I, E>(entries: I, extensions: E) -> Self
    where
        I: IntoIterator<Item = PathBuf>,
        E: IntoIterator<Item = String>,
    {
        Self {
            path_entries: entries.into_iter().collect(),
            command_extensions: extensions.into_iter().collect(),
            environment_names: BTreeSet::new(),
        }
    }

    pub fn with_environment_names<I, S>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.environment_names = names.into_iter().map(Into::into).collect();
        self
    }

    pub fn from_system() -> Self {
        let path = env::var_os("PATH").unwrap_or_default();
        let mut context = Self::with_path(env::split_paths(&path));
        if cfg!(windows) {
            context.command_extensions = env::var_os("PATHEXT")
                .map(|value| {
                    value
                        .to_string_lossy()
                        .split(';')
                        .filter(|extension| !extension.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .filter(|extensions: &Vec<String>| !extensions.is_empty())
                .unwrap_or_else(default_command_extensions);
        }
        context.environment_names = env::vars_os()
            .filter_map(|(name, _)| name.into_string().ok())
            .collect();
        context
    }

    fn contains_environment_name(&self, name: &str) -> bool {
        if cfg!(windows) {
            self.environment_names
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(name))
        } else {
            self.environment_names.contains(name)
        }
    }
}

fn default_command_extensions() -> Vec<String> {
    if cfg!(windows) {
        [".COM", ".EXE", ".BAT", ".CMD"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    } else {
        Vec::new()
    }
}

#[derive(Debug, Error)]
pub enum DoctorError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid JSON or JSONC in {path}: {message}")]
    InvalidJson { path: PathBuf, message: String },
    #[error("invalid TOML in {path}: {message}")]
    InvalidToml { path: PathBuf, message: String },
    #[error("unsupported MCP configuration in {path}: {message}")]
    UnsupportedConfig { path: PathBuf, message: String },
    #[error("invalid server entry {server} in {path}: {message}")]
    InvalidServer {
        path: PathBuf,
        server: String,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingCode {
    CommandMissing,
    CommandNotFound,
    CommandNotExecutable,
    RelativeCommandPath,
    CwdNotFound,
    RelativeCwd,
    PathContext,
    Placeholder,
    EmptyEnv,
    ServerNameConflict,
    DisabledServer,
    UnsupportedTransport,
    BearerTokenEnvMissing,
}

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub code: FindingCode,
    pub severity: Severity,
    pub server: Option<String>,
    pub location: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ServerReport {
    pub name: String,
    pub transport: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileReport {
    #[serde(serialize_with = "serialize_path")]
    pub path: PathBuf,
    pub servers: Vec<ServerReport>,
    pub findings: Vec<Finding>,
}

fn serialize_path<S>(path: &Path, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&path.to_string_lossy())
}

impl FileReport {
    pub fn error_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == Severity::Error)
            .count()
    }
}

/// Add warnings when inspected stdio configurations declare the same server
/// name, including names that differ only by letter case.
pub fn annotate_server_name_conflicts(files: &mut [FileReport]) {
    let mut occurrences: BTreeMap<String, Vec<(usize, String)>> = BTreeMap::new();
    for (file_index, file) in files.iter().enumerate() {
        for server in &file.servers {
            occurrences
                .entry(server.name.to_lowercase())
                .or_default()
                .push((file_index, server.name.clone()));
        }
    }

    for conflicts in occurrences.into_values().filter(|items| items.len() > 1) {
        for (file_index, server_name) in conflicts {
            let file = &mut files[file_index];
            let already_reported = file.findings.iter().any(|finding| {
                finding.code == FindingCode::ServerNameConflict
                    && finding.server.as_deref() == Some(server_name.as_str())
            });
            if already_reported {
                continue;
            }
            file.findings.push(Finding {
                code: FindingCode::ServerNameConflict,
                severity: Severity::Warning,
                server: Some(server_name),
                location: "server_name".to_string(),
                message: "server name appears in multiple inspected configuration entries; client precedence may shadow one definition".to_string(),
            });
        }
    }
}

/// Inspect one configuration file without launching any configured command.
pub fn inspect_file(path: &Path, context: &CheckContext) -> Result<FileReport, DoctorError> {
    inspect_file_with_workspace(path, context, None)
}

/// Inspect one configuration file and include Claude Code's local scope for
/// the supplied workspace without inspecting other project entries.
pub fn inspect_file_for_workspace(
    path: &Path,
    context: &CheckContext,
    workspace: &Path,
) -> Result<FileReport, DoctorError> {
    inspect_file_with_workspace(path, context, Some(workspace))
}

fn inspect_file_with_workspace(
    path: &Path,
    context: &CheckContext,
    workspace: Option<&Path>,
) -> Result<FileReport, DoctorError> {
    let bytes = fs::read(path).map_err(|source| DoctorError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let text = String::from_utf8(bytes)
        .map_err(|source| invalid_syntax_error(path, source.to_string()))?;
    let document = parse_document(path, &text)?;
    inspect_document(path, &document, context, workspace)
}

fn parse_document(path: &Path, text: &str) -> Result<Value, DoctorError> {
    if is_toml_config(path) {
        let document =
            toml::from_str::<toml::Table>(text).map_err(|source| DoctorError::InvalidToml {
                path: path.to_path_buf(),
                message: sanitized_toml_error_message(text, &source),
            })?;
        return serde_json::to_value(document).map_err(|source| DoctorError::InvalidToml {
            path: path.to_path_buf(),
            message: source.to_string(),
        });
    }

    // Windows editors may prefix JSON settings files with a UTF-8 BOM. JSONC
    // clients commonly accept it, so remove it before syntax checks without
    // changing the parsed configuration or exposing any values.
    let json_text = text.strip_prefix('\u{feff}').unwrap_or(text);

    if contains_unsupported_single_quote(json_text) {
        return Err(DoctorError::InvalidJson {
            path: path.to_path_buf(),
            message: "single-quoted strings are not valid JSONC".to_string(),
        });
    }
    jsonc_parser::parse_to_serde_value(json_text, &jsonc_parse_options())
        .map_err(|source| DoctorError::InvalidJson {
            path: path.to_path_buf(),
            message: source.to_string(),
        })?
        .ok_or_else(|| DoctorError::InvalidJson {
            path: path.to_path_buf(),
            message: "configuration is empty".to_string(),
        })
}

fn sanitized_toml_error_message(text: &str, source: &toml::de::Error) -> String {
    let Some(span) = source.span() else {
        return source.message().to_string();
    };
    let mut line = 1;
    let mut column = 1;
    for (offset, character) in text.char_indices() {
        if offset >= span.start {
            break;
        }
        if character == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    format!("{} at line {line}, column {column}", source.message())
}

fn invalid_syntax_error(path: &Path, message: String) -> DoctorError {
    if is_toml_config(path) {
        DoctorError::InvalidToml {
            path: path.to_path_buf(),
            message,
        }
    } else {
        DoctorError::InvalidJson {
            path: path.to_path_buf(),
            message,
        }
    }
}

fn is_toml_config(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
}

fn is_codex_plugin_cache_config(path: &Path) -> bool {
    if path.file_name() != Some(OsStr::new(".mcp.json")) {
        return false;
    }
    let components = path
        .components()
        .map(|component| component.as_os_str())
        .collect::<Vec<_>>();
    components.windows(3).any(|window| {
        window[0] == OsStr::new(".codex")
            && window[1] == OsStr::new("plugins")
            && window[2] == OsStr::new("cache")
    })
}

fn jsonc_parse_options() -> jsonc_parser::ParseOptions {
    jsonc_parser::ParseOptions {
        allow_comments: true,
        allow_loose_object_property_names: false,
        allow_trailing_commas: true,
    }
}

fn contains_unsupported_single_quote(text: &str) -> bool {
    let mut characters = text.chars().peekable();
    let mut in_double_quoted_string = false;
    let mut escaped = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some(character) = characters.next() {
        if in_line_comment {
            if character == '\n' || character == '\r' {
                in_line_comment = false;
            }
            continue;
        }
        if in_block_comment {
            if character == '*' && characters.peek() == Some(&'/') {
                characters.next();
                in_block_comment = false;
            }
            continue;
        }
        if in_double_quoted_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_double_quoted_string = false;
            }
            continue;
        }

        match character {
            '"' => in_double_quoted_string = true,
            '/' if characters.peek() == Some(&'/') => {
                characters.next();
                in_line_comment = true;
            }
            '/' if characters.peek() == Some(&'*') => {
                characters.next();
                in_block_comment = true;
            }
            '\'' => return true,
            _ => {}
        }
    }

    false
}

fn inspect_document(
    path: &Path,
    document: &Value,
    context: &CheckContext,
    workspace: Option<&Path>,
) -> Result<FileReport, DoctorError> {
    let root = document
        .as_object()
        .ok_or_else(|| DoctorError::UnsupportedConfig {
            path: path.to_path_buf(),
            message: "the top-level JSON value must be an object".to_string(),
        })?;
    let server_maps = server_maps(root, path, workspace)?;
    let plugin_cache = is_codex_plugin_cache_config(path);

    let mut report = FileReport {
        path: path.to_path_buf(),
        servers: Vec::new(),
        findings: Vec::new(),
    };
    for servers in server_maps {
        for (name, value) in servers {
            let server = value
                .as_object()
                .ok_or_else(|| DoctorError::InvalidServer {
                    path: path.to_path_buf(),
                    server: name.clone(),
                    message: "server entry must be an object".to_string(),
                })?;
            if is_toml_config(path) {
                match server.get("enabled") {
                    Some(Value::Bool(false)) => {
                        report.findings.push(Finding {
                            code: FindingCode::DisabledServer,
                            severity: Severity::Info,
                            server: Some(name.clone()),
                            location: "enabled".to_string(),
                            message: "Codex marks this server disabled; third-party MCP discovery tools may not honor that flag".to_string(),
                        });
                        continue;
                    }
                    None | Some(Value::Bool(true)) => {}
                    Some(_) => {
                        return Err(DoctorError::InvalidServer {
                            path: path.to_path_buf(),
                            server: name.clone(),
                            message: "enabled must be a boolean".to_string(),
                        });
                    }
                }
            }
            let transport = match server.get("type") {
                None => "stdio",
                Some(Value::String(value)) => value.as_str(),
                Some(_) => {
                    return Err(DoctorError::InvalidServer {
                        path: path.to_path_buf(),
                        server: name.clone(),
                        message: "type must be a string".to_string(),
                    });
                }
            };
            if transport != "stdio" || server.get("url").is_some() {
                if is_toml_config(path) {
                    inspect_codex_remote_auth(path, name, server, context, &mut report.findings)?;
                }
                report.findings.push(Finding {
                    code: FindingCode::UnsupportedTransport,
                    severity: Severity::Warning,
                    server: Some(name.clone()),
                    location: "type".to_string(),
                    message: "remote or non-stdio transport is outside this release".to_string(),
                });
                continue;
            }

            let command = match server.get("command") {
                None => None,
                Some(Value::String(value)) => Some(value.as_str()),
                Some(_) => {
                    return Err(DoctorError::InvalidServer {
                        path: path.to_path_buf(),
                        server: name.clone(),
                        message: "command must be a string".to_string(),
                    });
                }
            };
            let args = parse_string_array(server.get("args"), path, name, "args")?;
            let cwd = parse_optional_string(server.get("cwd"), path, name, "cwd")?;
            let env_map = parse_string_map(server.get("env"), path, name)?;
            if is_toml_config(path) {
                validate_codex_env_vars(server.get("env_vars"), path, name)?;
            }
            report.servers.push(ServerReport {
                name: name.clone(),
                transport: "stdio",
            });
            ServerCheck {
                server: name,
                command,
                args: &args,
                cwd: cwd.as_deref(),
                env_map: &env_map,
                context,
                plugin_cache,
            }
            .run(&mut report.findings);
        }
    }
    Ok(report)
}

fn inspect_codex_remote_auth(
    path: &Path,
    server_name: &str,
    server: &Map<String, Value>,
    context: &CheckContext,
    findings: &mut Vec<Finding>,
) -> Result<(), DoctorError> {
    let Some(value) = server.get("bearer_token_env_var") else {
        return Ok(());
    };
    let environment_name = match value {
        Value::String(name) if !name.trim().is_empty() => name,
        Value::String(_) => {
            return Err(DoctorError::InvalidServer {
                path: path.to_path_buf(),
                server: server_name.to_string(),
                message: "bearer_token_env_var must not be empty".to_string(),
            });
        }
        _ => {
            return Err(DoctorError::InvalidServer {
                path: path.to_path_buf(),
                server: server_name.to_string(),
                message: "bearer_token_env_var must be a string".to_string(),
            });
        }
    };

    if !context.contains_environment_name(environment_name) {
        findings.push(Finding {
            code: FindingCode::BearerTokenEnvMissing,
            severity: Severity::Warning,
            server: Some(server_name.to_string()),
            location: "bearer_token_env_var".to_string(),
            message: "configured bearer-token environment variable is absent from this process; set it before starting the MCP client".to_string(),
        });
    }

    Ok(())
}

fn server_maps<'a>(
    root: &'a Map<String, Value>,
    path: &Path,
    workspace: Option<&Path>,
) -> Result<Vec<&'a Map<String, Value>>, DoctorError> {
    let mut maps = Vec::new();
    if is_toml_config(path) {
        if let Some(value) = root.get("mcp_servers") {
            maps.push(object_map(value, path, "mcp_servers")?);
        }
        return Ok(maps);
    }
    if let Some(value) = root.get("mcpServers") {
        maps.push(object_map(value, path, "mcpServers")?);
    } else if let Some(value) = root.get("servers") {
        maps.push(object_map(value, path, "servers")?);
    } else if root.get("customizations").is_some() {
        maps.push(devcontainer_server_map(root, path)?);
    }

    if is_claude_code_state(path) {
        if let Some(workspace) = workspace {
            if let Some(local_servers) = claude_code_local_servers(root, path, workspace)? {
                maps.push(local_servers);
            }
        }
    }

    if maps.is_empty() && !is_claude_code_state(path) {
        return Err(DoctorError::UnsupportedConfig {
            path: path.to_path_buf(),
            message: "expected a top-level mcpServers or servers object, or customizations.vscode.mcp.servers".to_string(),
        });
    }
    Ok(maps)
}

fn devcontainer_server_map<'a>(
    root: &'a Map<String, Value>,
    path: &Path,
) -> Result<&'a Map<String, Value>, DoctorError> {
    let Some(customizations) = root.get("customizations") else {
        return Err(DoctorError::UnsupportedConfig {
            path: path.to_path_buf(),
            message: "expected a top-level mcpServers or servers object, or customizations.vscode.mcp.servers".to_string(),
        });
    };
    let customizations =
        customizations
            .as_object()
            .ok_or_else(|| DoctorError::UnsupportedConfig {
                path: path.to_path_buf(),
                message: "customizations must be an object".to_string(),
            })?;
    let Some(vscode) = customizations.get("vscode") else {
        return Err(DoctorError::UnsupportedConfig {
            path: path.to_path_buf(),
            message: "expected customizations.vscode.mcp.servers object".to_string(),
        });
    };
    let vscode = vscode
        .as_object()
        .ok_or_else(|| DoctorError::UnsupportedConfig {
            path: path.to_path_buf(),
            message: "customizations.vscode must be an object".to_string(),
        })?;
    let Some(mcp) = vscode.get("mcp") else {
        return Err(DoctorError::UnsupportedConfig {
            path: path.to_path_buf(),
            message: "expected customizations.vscode.mcp.servers object".to_string(),
        });
    };
    let mcp = mcp
        .as_object()
        .ok_or_else(|| DoctorError::UnsupportedConfig {
            path: path.to_path_buf(),
            message: "customizations.vscode.mcp must be an object".to_string(),
        })?;
    let Some(servers) = mcp.get("servers") else {
        return Err(DoctorError::UnsupportedConfig {
            path: path.to_path_buf(),
            message: "expected customizations.vscode.mcp.servers object".to_string(),
        });
    };
    object_map(servers, path, "customizations.vscode.mcp.servers")
}

fn is_claude_code_state(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == ".claude.json")
}

fn claude_code_local_servers<'a>(
    root: &'a Map<String, Value>,
    path: &Path,
    workspace: &Path,
) -> Result<Option<&'a Map<String, Value>>, DoctorError> {
    let Some(projects) = root.get("projects") else {
        return Ok(None);
    };
    let projects = projects
        .as_object()
        .ok_or_else(|| DoctorError::UnsupportedConfig {
            path: path.to_path_buf(),
            message: "projects must be an object".to_string(),
        })?;
    let canonical_workspace = workspace.canonicalize().ok();
    let project = projects
        .get(workspace.to_string_lossy().as_ref())
        .or_else(|| {
            canonical_workspace
                .as_ref()
                .and_then(|canonical_workspace| {
                    projects.iter().find_map(|(project_path, project)| {
                        Path::new(project_path)
                            .canonicalize()
                            .ok()
                            .filter(|canonical_project| canonical_project == canonical_workspace)
                            .map(|_| project)
                    })
                })
        });
    let Some(project) = project else {
        return Ok(None);
    };
    let project = project
        .as_object()
        .ok_or_else(|| DoctorError::UnsupportedConfig {
            path: path.to_path_buf(),
            message: "the current Claude Code project entry must be an object".to_string(),
        })?;
    let Some(servers) = project.get("mcpServers") else {
        return Ok(None);
    };
    object_map(servers, path, "projects.<current-workspace>.mcpServers").map(Some)
}

fn object_map<'a>(
    value: &'a Value,
    path: &Path,
    key: &'static str,
) -> Result<&'a Map<String, Value>, DoctorError> {
    value
        .as_object()
        .ok_or_else(|| DoctorError::UnsupportedConfig {
            path: path.to_path_buf(),
            message: format!("{key} must be an object"),
        })
}

fn parse_string_array(
    value: Option<&Value>,
    path: &Path,
    server: &str,
    field: &str,
) -> Result<Vec<String>, DoctorError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let array = value.as_array().ok_or_else(|| DoctorError::InvalidServer {
        path: path.to_path_buf(),
        server: server.to_string(),
        message: format!("{field} must be an array of strings"),
    })?;
    array
        .iter()
        .enumerate()
        .map(|(index, item)| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| DoctorError::InvalidServer {
                    path: path.to_path_buf(),
                    server: server.to_string(),
                    message: format!("{field}[{index}] must be a string"),
                })
        })
        .collect()
}

fn validate_codex_env_vars(
    value: Option<&Value>,
    path: &Path,
    server: &str,
) -> Result<(), DoctorError> {
    let Some(value) = value else { return Ok(()) };
    let array = value.as_array().ok_or_else(|| DoctorError::InvalidServer {
        path: path.to_path_buf(),
        server: server.to_string(),
        message: "env_vars must be an array".to_string(),
    })?;
    for (index, item) in array.iter().enumerate() {
        if item.is_string() {
            continue;
        }
        let table = item.as_object().ok_or_else(|| DoctorError::InvalidServer {
            path: path.to_path_buf(),
            server: server.to_string(),
            message: format!("env_vars[{index}] must be a string or a name/source table"),
        })?;
        if !table.get("name").is_some_and(Value::is_string) {
            return Err(DoctorError::InvalidServer {
                path: path.to_path_buf(),
                server: server.to_string(),
                message: format!("env_vars[{index}].name must be a string"),
            });
        }
        let source = table.get("source").and_then(Value::as_str);
        if !matches!(source, Some("local" | "remote")) {
            return Err(DoctorError::InvalidServer {
                path: path.to_path_buf(),
                server: server.to_string(),
                message: format!("env_vars[{index}].source must be local or remote"),
            });
        }
    }
    Ok(())
}

fn parse_optional_string(
    value: Option<&Value>,
    path: &Path,
    server: &str,
    field: &str,
) -> Result<Option<String>, DoctorError> {
    let Some(value) = value else { return Ok(None) };
    value
        .as_str()
        .map(|value| Some(value.to_owned()))
        .ok_or_else(|| DoctorError::InvalidServer {
            path: path.to_path_buf(),
            server: server.to_string(),
            message: format!("{field} must be a string"),
        })
}

fn parse_string_map(
    value: Option<&Value>,
    path: &Path,
    server: &str,
) -> Result<BTreeMap<String, String>, DoctorError> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let map = value
        .as_object()
        .ok_or_else(|| DoctorError::InvalidServer {
            path: path.to_path_buf(),
            server: server.to_string(),
            message: "env must be an object of strings".to_string(),
        })?;
    map.iter()
        .map(|(key, item)| {
            item.as_str()
                .map(|value| (key.clone(), value.to_owned()))
                .ok_or_else(|| DoctorError::InvalidServer {
                    path: path.to_path_buf(),
                    server: server.to_string(),
                    message: format!("env.{key} must be a string"),
                })
        })
        .collect()
}

struct ServerCheck<'a> {
    server: &'a str,
    command: Option<&'a str>,
    args: &'a [String],
    cwd: Option<&'a str>,
    env_map: &'a BTreeMap<String, String>,
    context: &'a CheckContext,
    plugin_cache: bool,
}

impl ServerCheck<'_> {
    fn run(&self, findings: &mut Vec<Finding>) {
        if self.command.is_none_or(|command| command.trim().is_empty()) {
            findings.push(finding(
                FindingCode::CommandMissing,
                Severity::Error,
                self.server,
                "command",
                "stdio server command is missing or empty",
            ));
        } else if let Some(command) = self.command {
            check_command(
                self.server,
                command,
                self.cwd,
                self.context,
                self.plugin_cache,
                findings,
            );
        }
        self.check_cwd(findings);
        self.check_args(findings);
        self.check_env(findings);
    }

    fn check_cwd(&self, findings: &mut Vec<Finding>) {
        let Some(cwd) = self.cwd else { return };
        if let Some(finding) = inspect_value(cwd, "cwd", Some(self.server), self.context) {
            findings.push(finding);
            return;
        }
        let cwd_path = Path::new(cwd);
        if cwd_path.is_relative() {
            findings.push(finding(
                FindingCode::RelativeCwd,
                Severity::Warning,
                self.server,
                "cwd",
                if self.plugin_cache {
                    "working directory is relative; a Codex plugin root or client may provide its base; use an absolute path for a deterministic check"
                } else {
                    "working directory is relative and its base depends on the client; use an absolute path for a deterministic check"
                },
            ));
            return;
        }
        if !cwd_path.is_dir() {
            findings.push(finding(
                FindingCode::CwdNotFound,
                Severity::Error,
                self.server,
                "cwd",
                "working directory does not exist",
            ));
        }
    }

    fn check_args(&self, findings: &mut Vec<Finding>) {
        for (index, arg) in self.args.iter().enumerate() {
            if let Some(finding) = inspect_value(
                arg,
                &format!("args[{index}]"),
                Some(self.server),
                self.context,
            ) {
                findings.push(finding);
            }
        }
    }

    fn check_env(&self, findings: &mut Vec<Finding>) {
        for (key, value) in self.env_map {
            let location = format!("env.{key}");
            if let Some(finding) = inspect_value(value, &location, Some(self.server), self.context)
            {
                findings.push(finding);
            } else if value.is_empty() {
                findings.push(finding(
                    FindingCode::EmptyEnv,
                    Severity::Warning,
                    self.server,
                    &location,
                    "environment variable is configured with an empty value",
                ));
            }
        }
    }
}

fn check_command(
    server: &str,
    command: &str,
    cwd: Option<&str>,
    context: &CheckContext,
    plugin_cache: bool,
    findings: &mut Vec<Finding>,
) {
    if let Some(finding) = inspect_value(command, "command", Some(server), context) {
        findings.push(finding);
        return;
    }
    let command_path = Path::new(command);
    if command_path.components().count() > 1 || command.contains('/') || command.contains('\\') {
        let resolved = if command_path.is_absolute() {
            command_path.to_path_buf()
        } else {
            let Some(cwd) = cwd else {
                findings.push(relative_command_finding(server, plugin_cache));
                return;
            };
            let cwd_path = Path::new(cwd);
            if !cwd_path.is_absolute() || placeholder_name(cwd).is_some() {
                findings.push(relative_command_finding(server, plugin_cache));
                return;
            }
            cwd_path.join(command_path)
        };
        if !resolved.is_file() {
            findings.push(finding(
                FindingCode::CommandNotFound,
                Severity::Error,
                server,
                "command",
                "explicit command path does not point to a file",
            ));
        } else if !is_executable(&resolved) {
            findings.push(finding(
                FindingCode::CommandNotExecutable,
                Severity::Error,
                server,
                "command",
                "explicit command path is not executable",
            ));
        }
        return;
    }

    if !context.path_entries.iter().any(|entry| {
        command_candidate(entry, command, &context.command_extensions)
            .is_some_and(|candidate| candidate.is_file() && is_executable(&candidate))
    }) {
        findings.push(finding(
            FindingCode::CommandNotFound,
            Severity::Error,
            server,
            "command",
            "command is not available on the current PATH",
        ));
    } else {
        findings.push(finding(
            FindingCode::PathContext,
            Severity::Warning,
            server,
            "command",
            "command exists on MCP Doctor's current PATH, but a GUI client may inherit a different PATH; prefer an absolute command path",
        ));
    }
}

fn relative_command_finding(server: &str, plugin_cache: bool) -> Finding {
    finding(
        FindingCode::RelativeCommandPath,
        Severity::Warning,
        server,
        "command",
        if plugin_cache {
            "command path is relative; a Codex plugin root or client may provide its base; use an absolute path for a deterministic check"
        } else {
            "command path is relative and its base depends on the client; use an absolute path for a deterministic check"
        },
    )
}

fn command_candidate(entry: &Path, command: &str, extensions: &[String]) -> Option<PathBuf> {
    let candidate = entry.join(command);
    if candidate.is_file() {
        return Some(candidate);
    }
    for suffix in extensions {
        let candidate = entry.join(format!("{command}{suffix}"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn finding(
    code: FindingCode,
    severity: Severity,
    server: &str,
    location: &str,
    message: &str,
) -> Finding {
    Finding {
        code,
        severity,
        server: Some(server.to_string()),
        location: location.to_string(),
        message: message.to_string(),
    }
}

fn placeholder_name(value: &str) -> Option<String> {
    for (start, end) in [("${", "}"), ("{{", "}}")] {
        if let Some(begin) = value.find(start) {
            let content_start = begin + start.len();
            if let Some(relative_end) = value[content_start..].find(end) {
                let raw = &value[content_start..content_start + relative_end];
                if let Some(name) = placeholder_token(raw) {
                    return Some(name);
                }
            }
        }
    }
    if let Some(begin) = value.find('%') {
        let content_start = begin + 1;
        if let Some(relative_end) = value[content_start..].find('%') {
            let raw = &value[content_start..content_start + relative_end];
            if valid_identifier(raw) {
                return Some(raw.to_string());
            }
        }
    }
    if let Some(begin) = value.find('$') {
        let name: String = value[begin + 1..]
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .collect();
        if valid_identifier(&name) {
            return Some(name);
        }
    }
    None
}

fn placeholder_token(raw: &str) -> Option<String> {
    let token = raw.split(":-").next().unwrap_or(raw);
    let mut parts = token.split(':');
    if parts.clone().all(valid_identifier) && parts.next().is_some() {
        Some(token.to_string())
    } else {
        None
    }
}

fn valid_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Inspect a single untrusted config value without resolving it.
pub fn inspect_value(
    value: &str,
    location: &str,
    server: Option<&str>,
    _context: &CheckContext,
) -> Option<Finding> {
    placeholder_name(value)
        .filter(|name| !name.starts_with("input:"))
        .map(|_| Finding {
            code: FindingCode::Placeholder,
            severity: Severity::Warning,
            server: server.map(str::to_owned),
            location: location.to_string(),
            message: "unresolved environment placeholder; the configured value was not emitted"
                .to_string(),
        })
}

/// Return existing conventional config paths for the current workspace and user.
pub fn discover_paths(workspace: &Path) -> Vec<PathBuf> {
    let home = home_dir();
    let app_data = env::var_os("APPDATA").map(PathBuf::from);
    discover_paths_from_roots(workspace, home.as_deref(), app_data.as_deref())
}

fn discover_paths_from_roots(
    workspace: &Path,
    home: Option<&Path>,
    app_data: Option<&Path>,
) -> Vec<PathBuf> {
    let mut candidates = vec![
        workspace.join(".codex/config.toml"),
        workspace.join(".devcontainer/devcontainer.json"),
        workspace.join(".vscode/mcp.json"),
        workspace.join(".mcp.json"),
        workspace.join(".github/mcp.json"),
        workspace.join(".github/mcp-config.json"),
        workspace.join(".cursor/mcp.json"),
    ];
    if let Some(home) = home {
        candidates.push(home.join(".codex/config.toml"));
        candidates.extend(discover_codex_plugin_configs(home));
        candidates.extend([
            home.join(".claude.json"),
            home.join(".copilot/mcp-config.json"),
            home.join(".cursor/mcp.json"),
            home.join(".config/Claude/claude_desktop_config.json"),
            home.join(".config/Code/User/mcp.json"),
            home.join(
                ".config/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
            ),
            home.join("Library/Application Support/Claude/claude_desktop_config.json"),
            home.join("Library/Application Support/Code/User/mcp.json"),
            home.join(
                "Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
            ),
        ]);
    }
    if let Some(app_data) = app_data {
        candidates.extend([
            app_data.join("Claude/claude_desktop_config.json"),
            app_data.join("Code/User/mcp.json"),
            app_data.join(
                "Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
            ),
        ]);
    }
    let mut seen = BTreeSet::new();
    candidates
        .into_iter()
        .filter(|path| path.is_file() && seen.insert(path.clone()))
        .collect()
}

const MAX_CODEX_PLUGIN_CONFIGS: usize = 128;

fn discover_codex_plugin_configs(home: &Path) -> Vec<PathBuf> {
    let cache = home.join(".codex/plugins/cache");
    let mut configs = Vec::new();
    for marketplace in sorted_child_directories(&cache) {
        for plugin in sorted_child_directories(&marketplace) {
            for version in sorted_child_directories(&plugin) {
                let config = version.join(".mcp.json");
                if config.is_file() {
                    configs.push(config);
                    if configs.len() == MAX_CODEX_PLUGIN_CONFIGS {
                        return configs;
                    }
                }
            }
        }
    }
    configs
}

fn sorted_child_directories(path: &Path) -> Vec<PathBuf> {
    let mut directories = fs::read_dir(path)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let file_type = entry.file_type().ok()?;
            file_type.is_dir().then(|| entry.path())
        })
        .collect::<Vec<_>>();
    directories.sort();
    directories
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        env::var_os("HOME").map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn parses_mcp_servers_and_checks_command_without_exposing_env_values() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("mcp.json");
        fs::write(
            &config,
            r#"{"mcpServers":{"demo":{"command":"node","args":["server.js","${API_KEY}"],"env":{"API_KEY":"super-secret"}}}}"#,
        )
        .expect("write config");

        let report = inspect_file(
            &config,
            &CheckContext::with_path([dir.path().to_path_buf()]),
        )
        .expect("inspect config");

        assert_eq!(report.servers.len(), 1);
        assert_eq!(report.servers[0].name, "demo");
        assert!(report.findings.iter().any(|finding| {
            finding.code == FindingCode::Placeholder
                && !finding.message.contains("API_KEY")
                && !finding.message.contains("super-secret")
        }));
        let serialized = serde_json::to_string(&report).expect("serialize report");
        assert!(!serialized.contains("super-secret"));
    }

    #[test]
    fn accepts_utf8_bom_in_json_configuration() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("mcp.json");
        let mut bytes = vec![0xef, 0xbb, 0xbf];
        bytes.extend_from_slice(br#"{"mcpServers":{"demo":{"command":"missing-mcp-command"}}}"#);
        fs::write(&config, bytes).expect("write config");

        let report = inspect_file(&config, &CheckContext::default()).expect("inspect config");

        assert_eq!(report.servers.len(), 1);
        assert_eq!(report.servers[0].name, "demo");
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == FindingCode::CommandNotFound)
        );
    }

    #[test]
    fn parses_codex_stdio_servers_without_exposing_environment_values() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("config.toml");
        let missing_cwd = serde_json::to_string(
            &dir.path()
                .join("missing-mcp-doctor-directory")
                .to_string_lossy(),
        )
        .expect("encode missing cwd");
        fs::write(
            &config,
            r#"
                [mcp_servers.demo]
                command = "missing-codex-command"
                args = ["--token", "${API_KEY}"]
                cwd = __MISSING_CWD__
                env_vars = [
                    "PASSTHROUGH_SECRET",
                    { name = "REMOTE_SECRET", source = "remote" },
                ]

                [mcp_servers.demo.env]
                API_KEY = "super-secret"
            "#
            .replace("__MISSING_CWD__", &missing_cwd),
        )
        .expect("write config");

        let report = inspect_file(&config, &CheckContext::with_path(Vec::<PathBuf>::new()))
            .expect("inspect Codex config");

        assert_eq!(report.servers.len(), 1);
        assert_eq!(report.servers[0].name, "demo");
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == FindingCode::CommandNotFound)
        );
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == FindingCode::CwdNotFound)
        );
        assert!(report.findings.iter().any(|finding| {
            finding.code == FindingCode::Placeholder
                && !finding.message.contains("API_KEY")
                && !finding.message.contains("super-secret")
        }));
        let serialized = serde_json::to_string(&report).expect("serialize report");
        assert!(!serialized.contains("super-secret"));
        assert!(!serialized.contains("PASSTHROUGH_SECRET"));
        assert!(!serialized.contains("REMOTE_SECRET"));
    }

    #[test]
    fn rejects_invalid_codex_env_vars_without_echoing_values() {
        let cases = [
            ("\"never-print-this\"", "env_vars must be an array"),
            (
                "[42]",
                "env_vars[0] must be a string or a name/source table",
            ),
            (
                "[{ source = \"remote\" }]",
                "env_vars[0].name must be a string",
            ),
            (
                "[{ name = \"TOKEN\" }]",
                "env_vars[0].source must be local or remote",
            ),
            (
                "[{ name = \"TOKEN\", source = \"never-print-this\" }]",
                "env_vars[0].source must be local or remote",
            ),
        ];

        for (env_vars, expected_message) in cases {
            let dir = tempdir().expect("tempdir");
            let config = dir.path().join("config.toml");
            let contents =
                format!("[mcp_servers.demo]\ncommand = \"node\"\nenv_vars = {env_vars}\n");
            fs::write(&config, contents).expect("write config");

            let error = inspect_file(&config, &CheckContext::default())
                .expect_err("invalid env_vars declaration");
            let message = error.to_string();

            assert!(message.contains(expected_message), "{message}");
            assert!(!message.contains("never-print-this"));
        }
    }

    #[test]
    fn reports_disabled_codex_servers_and_warns_about_remote_servers() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("config.toml");
        fs::write(
            &config,
            r#"
                [mcp_servers.disabled]
                command = "missing-disabled-command"
                enabled = false

                [mcp_servers.remote]
                url = "https://example.test/mcp"

                [mcp_servers.active]
                command = "missing-active-command"
            "#,
        )
        .expect("write config");

        let report = inspect_file(&config, &CheckContext::with_path(Vec::<PathBuf>::new()))
            .expect("inspect Codex config");

        assert_eq!(report.servers.len(), 1);
        assert_eq!(report.servers[0].name, "active");
        assert!(report.findings.iter().any(|finding| {
            finding.code == FindingCode::UnsupportedTransport
                && finding.server.as_deref() == Some("remote")
        }));
        assert!(report.findings.iter().any(|finding| {
            finding.code == FindingCode::DisabledServer
                && finding.server.as_deref() == Some("disabled")
        }));
    }

    #[test]
    fn disabled_codex_server_does_not_receive_launch_checks() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("config.toml");
        fs::write(
            &config,
            r#"
                [mcp_servers.disabled]
                command = "missing-command-that-must-not-be-checked"
                cwd = "/path/that-must-not-be-checked"
                enabled = false
            "#,
        )
        .expect("write config");

        let report = inspect_file(&config, &CheckContext::default()).expect("inspect config");

        assert!(report.servers.is_empty());
        assert!(report.findings.iter().any(|finding| {
            finding.code == FindingCode::DisabledServer
                && finding.severity == Severity::Info
                && finding.location == "enabled"
        }));
        assert!(report.findings.iter().all(|finding| {
            !matches!(
                finding.code,
                FindingCode::CommandNotFound
                    | FindingCode::CwdNotFound
                    | FindingCode::RelativeCommandPath
                    | FindingCode::RelativeCwd
            )
        }));
    }

    #[test]
    fn rejects_non_boolean_codex_server_enablement() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("config.toml");
        fs::write(
            &config,
            r#"
                [mcp_servers.demo]
                command = "node"
                enabled = "sometimes"
            "#,
        )
        .expect("write config");

        let error =
            inspect_file(&config, &CheckContext::default()).expect_err("invalid enabled value");

        assert!(error.to_string().contains("enabled must be a boolean"));
    }

    #[test]
    fn rejects_duplicate_codex_server_tables_as_invalid_toml() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("config.toml");
        fs::write(
            &config,
            r#"
                [mcp_servers.demo]
                command = "first"

                [mcp_servers.demo]
                command = "second"
            "#,
        )
        .expect("write config");

        let error = inspect_file(&config, &CheckContext::default()).expect_err("invalid TOML");

        assert!(error.to_string().contains("invalid TOML"));
    }

    #[test]
    fn rejects_unescaped_windows_paths_as_invalid_toml() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("config.toml");
        fs::write(
            &config,
            r#"
                [mcp_servers.demo]
                command = "powershell"
                args = ["C:\codex\server.ps1"]
            "#,
        )
        .expect("write config");

        let error = inspect_file(&config, &CheckContext::default()).expect_err("invalid TOML");

        assert!(error.to_string().contains("invalid TOML"));
    }

    #[test]
    fn invalid_toml_errors_do_not_echo_source_lines() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("config.toml");
        fs::write(
            &config,
            r#"
                [mcp_servers.demo.env]
                TOKEN = "never-print-this\q"
            "#,
        )
        .expect("write config");

        let error = inspect_file(&config, &CheckContext::default()).expect_err("invalid TOML");
        let message = error.to_string();

        assert!(message.contains("invalid TOML"));
        assert!(message.contains("line"));
        assert!(!message.contains("never-print-this"));
    }

    #[test]
    fn accepts_codex_config_without_top_level_mcp_servers() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("config.toml");
        fs::write(
            &config,
            r#"
                model = "gpt-5.2-codex"

                [plugins."sample@test".mcp_servers.sample]
                enabled = true
            "#,
        )
        .expect("write config");

        let report = inspect_file(&config, &CheckContext::default())
            .expect("inspect Codex config without MCP servers");

        assert!(report.servers.is_empty());
        assert!(report.findings.is_empty());
    }

    #[test]
    fn parses_vscode_devcontainer_servers_without_exposing_env_values() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("devcontainer.json");
        fs::write(
            &config,
            r#"{
                // VS Code Dev Container MCP configuration
                "customizations": {
                    "vscode": {
                        "mcp": {
                            "servers": {
                                "playwright": {
                                    "command": "missing-playwright",
                                    "env": {"TOKEN": "super-secret",},
                                },
                                "remote": {
                                    "type": "http",
                                    "url": "https://example.test/mcp",
                                },
                            },
                        },
                    },
                },
            }"#,
        )
        .expect("write config");

        let report = inspect_file(&config, &CheckContext::with_path(Vec::<PathBuf>::new()))
            .expect("inspect config");

        assert_eq!(report.servers.len(), 1);
        assert_eq!(report.servers[0].name, "playwright");
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == FindingCode::CommandNotFound)
        );
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == FindingCode::UnsupportedTransport)
        );
        let serialized = serde_json::to_string(&report).expect("serialize report");
        assert!(!serialized.contains("super-secret"));
    }

    #[test]
    fn reports_missing_command_in_devcontainer_server() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("devcontainer.json");
        fs::write(
            &config,
            r#"{"customizations":{"vscode":{"mcp":{"servers":{"demo":{"args":[]}}}}}}"#,
        )
        .expect("write config");

        let report = inspect_file(&config, &CheckContext::with_path(Vec::<PathBuf>::new()))
            .expect("inspect config");

        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == FindingCode::CommandMissing)
        );
    }

    #[test]
    fn rejects_malformed_devcontainer_server_map_with_stable_error() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("devcontainer.json");
        fs::write(
            &config,
            r#"{"customizations":{"vscode":{"mcp":{"servers":[]}}}}"#,
        )
        .expect("write config");

        let error = inspect_file(&config, &CheckContext::default()).expect_err("invalid config");
        let message = error.to_string();
        assert!(message.contains("unsupported MCP configuration"));
        assert!(message.contains("customizations.vscode.mcp.servers must be an object"));
    }

    #[test]
    fn reports_missing_bare_command_using_supplied_path() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("mcp.json");
        fs::write(
            &config,
            r#"{"mcpServers":{"demo":{"command":"definitely-not-installed"}}}"#,
        )
        .expect("write config");

        let report =
            inspect_file(&config, &CheckContext::with_path(Vec::new())).expect("inspect config");

        assert!(report.findings.iter().any(|finding| {
            finding.code == FindingCode::CommandNotFound
                && finding.server.as_deref() == Some("demo")
        }));
    }

    #[cfg(unix)]
    #[test]
    fn recognizes_an_executable_on_path_without_running_it() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().expect("tempdir");
        let executable = dir.path().join("fake-mcp");
        let marker = dir.path().join("executed");
        fs::write(
            &executable,
            format!("#!/bin/sh\ntouch {}\n", marker.display()),
        )
        .expect("write executable");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).expect("chmod");
        let config = dir.path().join("mcp.json");
        fs::write(&config, r#"{"mcpServers":{"demo":{"command":"fake-mcp"}}}"#)
            .expect("write config");

        let report = inspect_file(
            &config,
            &CheckContext::with_path([dir.path().to_path_buf()]),
        )
        .expect("inspect config");

        assert!(
            !report
                .findings
                .iter()
                .any(|finding| finding.code == FindingCode::CommandNotFound)
        );
        assert!(!marker.exists());
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == FindingCode::PathContext)
        );
    }

    #[test]
    fn does_not_assume_a_base_for_relative_paths() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("mcp.json");
        fs::write(
            &config,
            r#"{"servers":{"demo":{"type":"stdio","command":"./missing-server","cwd":"relative-dir"}}}"#,
        )
        .expect("write config");

        let report =
            inspect_file(&config, &CheckContext::with_path(Vec::new())).expect("inspect config");

        assert!(!report.findings.iter().any(|finding| matches!(
            finding.code,
            FindingCode::CommandNotFound | FindingCode::CwdNotFound
        )));
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == FindingCode::RelativeCwd)
        );
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == FindingCode::RelativeCommandPath)
        );
    }

    #[test]
    fn skips_remote_servers_with_an_explicit_unsupported_finding() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("mcp.json");
        fs::write(
            &config,
            r#"{"servers":{"remote":{"type":"http","url":"https://example.test/mcp"}}}"#,
        )
        .expect("write config");

        let report =
            inspect_file(&config, &CheckContext::with_path(Vec::new())).expect("inspect config");

        assert!(report.servers.is_empty());
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == FindingCode::UnsupportedTransport)
        );
    }

    #[test]
    fn warns_when_codex_remote_bearer_token_environment_name_is_missing() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("config.toml");
        fs::write(
            &config,
            r#"
                [mcp_servers.remote]
                url = "https://example.test/mcp"
                bearer_token_env_var = "MCP_DOCTOR_REMOTE_AUTH_SENTINEL"
            "#,
        )
        .expect("write config");

        let report = inspect_file(
            &config,
            &CheckContext::with_path(Vec::<PathBuf>::new())
                .with_environment_names(Vec::<String>::new()),
        )
        .expect("inspect config");

        assert!(report.findings.iter().any(|finding| {
            finding.code == FindingCode::BearerTokenEnvMissing
                && finding.server.as_deref() == Some("remote")
                && finding.location == "bearer_token_env_var"
        }));
        let output = serde_json::to_string(&report).expect("serialize report");
        assert!(!output.contains("MCP_DOCTOR_REMOTE_AUTH_SENTINEL"));
    }

    #[test]
    fn accepts_present_codex_remote_bearer_token_environment_name() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("config.toml");
        fs::write(
            &config,
            r#"
                [mcp_servers.remote]
                url = "https://example.test/mcp"
                bearer_token_env_var = "MCP_DOCTOR_REMOTE_AUTH_SENTINEL"
            "#,
        )
        .expect("write config");

        let report = inspect_file(
            &config,
            &CheckContext::with_path(Vec::<PathBuf>::new())
                .with_environment_names(["MCP_DOCTOR_REMOTE_AUTH_SENTINEL"]),
        )
        .expect("inspect config");

        assert!(
            !report
                .findings
                .iter()
                .any(|finding| finding.code == FindingCode::BearerTokenEnvMissing)
        );
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == FindingCode::UnsupportedTransport)
        );
    }

    #[test]
    fn rejects_invalid_codex_remote_bearer_token_environment_declarations() {
        let cases = [
            (
                "bearer_token_env_var = \"\"",
                "bearer_token_env_var must not be empty",
            ),
            (
                "bearer_token_env_var = 42",
                "bearer_token_env_var must be a string",
            ),
        ];

        for (declaration, expected_message) in cases {
            let dir = tempdir().expect("tempdir");
            let config = dir.path().join("config.toml");
            fs::write(
                &config,
                format!(
                    "[mcp_servers.remote]\nurl = \"https://example.test/mcp\"\n{declaration}\n"
                ),
            )
            .expect("write config");

            let error = inspect_file(&config, &CheckContext::default())
                .expect_err("invalid bearer token environment declaration");

            assert!(error.to_string().contains(expected_message));
        }
    }

    #[test]
    fn missing_stdio_command_is_a_check_finding_not_a_file_error() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("mcp.json");
        fs::write(&config, r#"{"mcpServers":{"demo":{"args":[]}}}"#).expect("write config");

        let report =
            inspect_file(&config, &CheckContext::with_path(Vec::new())).expect("inspect config");

        assert_eq!(report.servers.len(), 1);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == FindingCode::CommandMissing)
        );
    }

    #[cfg(unix)]
    #[test]
    fn reports_relative_command_without_assuming_client_semantics() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().expect("tempdir");
        let bin_dir = dir.path().join("server-dir");
        fs::create_dir(&bin_dir).expect("create server dir");
        let executable = bin_dir.join("server");
        fs::write(&executable, "#!/bin/sh\nexit 0\n").expect("write executable");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).expect("chmod");
        let config = dir.path().join("mcp.json");
        fs::write(
            &config,
            r#"{"mcpServers":{"demo":{"command":"./server","cwd":"server-dir"}}}"#,
        )
        .expect("write config");

        let report =
            inspect_file(&config, &CheckContext::with_path(Vec::new())).expect("inspect config");

        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == FindingCode::RelativeCommandPath)
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolves_relative_command_against_an_absolute_working_directory() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().expect("tempdir");
        let executable = dir.path().join("server");
        fs::write(&executable, "#!/bin/sh\nexit 0\n").expect("write executable");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).expect("chmod");
        let config = dir.path().join("mcp.json");
        let document = serde_json::json!({
            "mcpServers": {
                "demo": {
                    "command": "./server",
                    "cwd": dir.path(),
                }
            }
        });
        fs::write(
            &config,
            serde_json::to_vec(&document).expect("serialize config"),
        )
        .expect("write config");

        let report =
            inspect_file(&config, &CheckContext::with_path(Vec::new())).expect("inspect config");

        assert!(!report.findings.iter().any(|finding| matches!(
            finding.code,
            FindingCode::RelativeCommandPath | FindingCode::CommandNotFound
        )));
    }

    #[test]
    fn detects_vscode_environment_placeholder_without_resolving_it() {
        let context = CheckContext::with_path(Vec::<PathBuf>::new());

        let finding = inspect_value("${env:API_KEY}", "env.API_KEY", None, &context)
            .expect("placeholder finding");

        assert_eq!(finding.code, FindingCode::Placeholder);
        assert!(!finding.message.contains("API_KEY"));
    }

    #[test]
    fn accepts_vscode_input_references_without_environment_warnings() {
        let context = CheckContext::with_path(Vec::<PathBuf>::new());

        assert!(inspect_value("${input:api-key}", "args[0]", None, &context).is_none());
        assert!(
            inspect_value("prefix-${input:api-key}-suffix", "args[0]", None, &context,).is_none()
        );
    }

    #[test]
    fn does_not_treat_percent_encoded_text_as_an_environment_placeholder() {
        let context = CheckContext::with_path(Vec::<PathBuf>::new());

        let finding = inspect_value("https://example.test/a%20%b", "args[0]", None, &context);

        assert!(finding.is_none());
    }

    #[test]
    fn invalid_json_is_a_structured_error() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("mcp.json");
        fs::write(&config, "{broken").expect("write config");

        let error =
            inspect_file(&config, &CheckContext::with_path(Vec::new())).expect_err("must fail");

        assert!(matches!(error, DoctorError::InvalidJson { .. }));
    }

    #[test]
    fn accepts_jsonc_comments_and_trailing_commas() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("mcp.json");
        fs::write(
            &config,
            r#"{
                /* Keep the user's server enabled for local integration tests. */
                "servers": {
                    "demo": {
                        "type": "stdio",
                        "command": "node",
                        "args": ["server's.js",], // Entry point.
                    },
                },
            }"#,
        )
        .expect("write config");

        let report = inspect_file(&config, &CheckContext::with_path(Vec::new()))
            .expect("JSONC config should parse");

        assert_eq!(report.servers.len(), 1);
        assert_eq!(report.servers[0].name, "demo");
    }

    #[test]
    fn rejects_json5_only_syntax_outside_jsonc_boundary() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("mcp.json");
        fs::write(
            &config,
            r#"{
                servers: {
                    demo: { command: 'node' },
                },
            }"#,
        )
        .expect("write config");

        let error =
            inspect_file(&config, &CheckContext::with_path(Vec::new())).expect_err("must fail");

        assert!(matches!(error, DoctorError::InvalidJson { .. }));
    }

    #[test]
    fn rejects_single_quoted_json5_strings() {
        let dir = tempdir().expect("tempdir");
        let config = dir.path().join("mcp.json");
        fs::write(&config, r#"{"servers":{"demo":{"command":'node'}}}"#).expect("write config");

        let error =
            inspect_file(&config, &CheckContext::with_path(Vec::new())).expect_err("must fail");

        assert!(matches!(error, DoctorError::InvalidJson { .. }));
    }

    #[test]
    fn env_values_are_never_used_for_placeholder_resolution() {
        let context = CheckContext::with_path(Vec::<PathBuf>::new());
        let finding = inspect_value(
            "prefix-$literal_secret_9427-suffix",
            "env.TOKEN",
            None,
            &context,
        )
        .expect("placeholder finding");

        assert_eq!(finding.code, FindingCode::Placeholder);
        assert!(!finding.message.contains("literal_secret_9427"));
        assert!(!finding.message.contains("prefix"));
    }

    #[cfg(unix)]
    #[test]
    fn custom_command_extensions_are_checked_without_execution() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().expect("tempdir");
        let executable = dir.path().join("demo.COM");
        fs::write(&executable, "not executed").expect("write executable");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).expect("chmod");
        let config = dir.path().join("mcp.json");
        fs::write(&config, r#"{"mcpServers":{"demo":{"command":"demo"}}}"#).expect("write config");
        let context = CheckContext::with_path_and_extensions(
            [dir.path().to_path_buf()],
            [".COM".to_string()],
        );

        let report = inspect_file(&config, &context).expect("inspect config");

        assert!(
            !report
                .findings
                .iter()
                .any(|finding| finding.code == FindingCode::CommandNotFound)
        );
    }

    #[cfg(windows)]
    #[test]
    fn default_windows_command_extensions_match_platform_search_defaults() {
        assert_eq!(
            default_command_extensions(),
            [".COM", ".EXE", ".BAT", ".CMD"].map(str::to_owned)
        );
    }

    #[test]
    fn discovers_repository_and_copilot_config_paths() {
        let workspace = tempdir().expect("workspace");
        let home = tempdir().expect("home");
        let app_data = tempdir().expect("app data");

        for relative in [
            ".codex/config.toml",
            ".devcontainer/devcontainer.json",
            ".vscode/mcp.json",
            ".mcp.json",
            ".github/mcp.json",
            ".github/mcp-config.json",
            ".cursor/mcp.json",
        ] {
            let path = workspace.path().join(relative);
            fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
            fs::write(path, "{}").expect("write workspace config");
        }
        let copilot = home.path().join(".copilot/mcp-config.json");
        fs::create_dir_all(copilot.parent().expect("parent")).expect("create parent");
        fs::write(&copilot, "{}").expect("write Copilot config");
        let claude_code = home.path().join(".claude.json");
        fs::write(&claude_code, "{}").expect("write Claude Code config");
        let codex_user = home.path().join(".codex/config.toml");
        fs::create_dir_all(codex_user.parent().expect("parent")).expect("create parent");
        fs::write(&codex_user, "").expect("write Codex config");
        let plugin_config = home
            .path()
            .join(".codex/plugins/cache/example-marketplace/example-plugin/1.0.0/.mcp.json");
        fs::create_dir_all(plugin_config.parent().expect("parent")).expect("create plugin path");
        fs::write(&plugin_config, "{}").expect("write plugin config");
        let vscode_user = home.path().join(".config/Code/User/mcp.json");
        fs::create_dir_all(vscode_user.parent().expect("parent")).expect("create parent");
        fs::write(&vscode_user, "{}").expect("write VS Code config");
        let windows_vscode_user = app_data.path().join("Code/User/mcp.json");
        fs::create_dir_all(windows_vscode_user.parent().expect("parent")).expect("create parent");
        fs::write(&windows_vscode_user, "{}").expect("write Windows VS Code config");

        let paths =
            discover_paths_from_roots(workspace.path(), Some(home.path()), Some(app_data.path()));

        assert_eq!(paths.len(), 13);
        assert!(paths.contains(&workspace.path().join(".codex/config.toml")));
        assert!(paths.contains(&workspace.path().join(".devcontainer/devcontainer.json")));
        assert!(paths.contains(&workspace.path().join(".github/mcp-config.json")));
        assert!(paths.contains(&workspace.path().join(".github/mcp.json")));
        assert!(paths.contains(&copilot));
        assert!(paths.contains(&claude_code));
        assert!(paths.contains(&codex_user));
        assert!(paths.contains(&plugin_config));
        assert!(paths.contains(&vscode_user));
        assert!(paths.contains(&windows_vscode_user));
    }

    #[test]
    fn bounds_codex_plugin_cache_discovery() {
        let home = tempdir().expect("home");
        for index in 0..=MAX_CODEX_PLUGIN_CONFIGS {
            let config = home.path().join(format!(
                ".codex/plugins/cache/example-marketplace/plugin-{index}/1.0.0/.mcp.json"
            ));
            fs::create_dir_all(config.parent().expect("plugin config parent"))
                .expect("create plugin config parent");
            fs::write(config, "{}").expect("write plugin config");
        }

        let paths = discover_codex_plugin_configs(home.path());

        assert_eq!(paths.len(), MAX_CODEX_PLUGIN_CONFIGS);
        assert!(paths.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn inspects_claude_code_user_and_current_workspace_scopes_only() {
        let dir = tempdir().expect("tempdir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("create workspace");
        let config = dir.path().join(".claude.json");
        fs::write(
            &config,
            serde_json::to_vec(&serde_json::json!({
                "mcpServers": {
                    "user-server": {"command": "missing-user-command"}
                },
                "projects": {
                    workspace.to_string_lossy(): {
                        "mcpServers": {
                            "local-server": {"command": "missing-local-command"}
                        }
                    },
                    "/another/project": {
                        "mcpServers": {
                            "user-server": {
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

        let mut report = inspect_file_for_workspace(
            &config,
            &CheckContext::with_path(Vec::<PathBuf>::new()),
            &workspace,
        )
        .expect("inspect Claude Code config");

        let names: BTreeSet<_> = report
            .servers
            .iter()
            .map(|server| server.name.as_str())
            .collect();
        assert_eq!(names, BTreeSet::from(["local-server", "user-server"]));
        assert_eq!(report.servers.len(), 2);
        let serialized = serde_json::to_string(&report).expect("serialize report");
        assert!(!serialized.contains("never-print-this"));
        annotate_server_name_conflicts(std::slice::from_mut(&mut report));
        assert!(
            report
                .findings
                .iter()
                .all(|finding| { finding.code != FindingCode::ServerNameConflict })
        );
    }

    #[test]
    fn annotates_case_insensitive_server_name_conflicts() {
        let mut files = vec![
            FileReport {
                path: PathBuf::from("user/mcp.json"),
                servers: vec![ServerReport {
                    name: "MCPBrowser".to_string(),
                    transport: "stdio",
                }],
                findings: Vec::new(),
            },
            FileReport {
                path: PathBuf::from(".mcp.json"),
                servers: vec![ServerReport {
                    name: "mcpbrowser".to_string(),
                    transport: "stdio",
                }],
                findings: Vec::new(),
            },
        ];

        annotate_server_name_conflicts(&mut files);

        assert!(files.iter().all(|file| {
            file.findings.iter().any(|finding| {
                finding.code == FindingCode::ServerNameConflict
                    && finding.severity == Severity::Warning
                    && finding
                        .message
                        .contains("multiple inspected configuration entries")
            })
        }));
    }

    #[test]
    fn leaves_unique_server_names_unchanged() {
        let mut files = vec![
            FileReport {
                path: PathBuf::from("one.json"),
                servers: vec![ServerReport {
                    name: "alpha".to_string(),
                    transport: "stdio",
                }],
                findings: Vec::new(),
            },
            FileReport {
                path: PathBuf::from("two.json"),
                servers: vec![ServerReport {
                    name: "beta".to_string(),
                    transport: "stdio",
                }],
                findings: Vec::new(),
            },
        ];

        annotate_server_name_conflicts(&mut files);

        assert!(files.iter().all(|file| file.findings.is_empty()));
    }
}
