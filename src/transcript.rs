//! Bounded, offline diagnostics for captured MCP stdio traffic.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use thiserror::Error;

const MAX_RECORD_BYTES: usize = 4 * 1024 * 1024;
const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_RECORDS: usize = 100_000;
const MAX_FINDINGS: usize = 10_000;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptDirection {
    ClientToServer,
    ServerToClient,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptSeverity {
    Error,
    Warning,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptFindingCode {
    TranscriptStdoutPollution,
    TranscriptInvalidJson,
    TranscriptMultilineMessage,
    TranscriptInvalidJsonrpcVersion,
    TranscriptInvalidJsonrpcMessage,
    TranscriptInitializeOrder,
    TranscriptDuplicateInitialize,
    TranscriptDiagnosticsTruncated,
}

#[derive(Clone, Debug, Serialize)]
pub struct TranscriptFinding {
    pub code: TranscriptFindingCode,
    pub severity: TranscriptSeverity,
    pub line: usize,
    pub direction: TranscriptDirection,
    pub message: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct TranscriptReport {
    #[serde(serialize_with = "serialize_path")]
    pub path: PathBuf,
    pub records: usize,
    pub findings: Vec<TranscriptFinding>,
}

#[derive(Debug, Error)]
pub enum TranscriptError {
    #[error("cannot read transcript {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid transcript record at line {line}: {message}")]
    InvalidRecord { line: usize, message: &'static str },
    #[error("transcript resource limit exceeded at line {line}: {message}")]
    LimitExceeded { line: usize, message: &'static str },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema_version: u8,
    direction: TranscriptDirection,
    payload: String,
}

#[derive(Default)]
struct LifecycleState {
    initialize_seen: bool,
    initialize_response_seen: bool,
    initialize_id: Option<String>,
}

pub fn inspect_transcript(path: &Path) -> Result<TranscriptReport, TranscriptError> {
    let file = File::open(path).map_err(|source| TranscriptError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    inspect_reader(path, BufReader::new(file))
}

fn inspect_reader<R: BufRead>(
    path: &Path,
    mut reader: R,
) -> Result<TranscriptReport, TranscriptError> {
    let mut report = TranscriptReport {
        path: path.to_path_buf(),
        records: 0,
        findings: Vec::new(),
    };
    let mut state = LifecycleState::default();
    let mut diagnostics_saturated = false;
    let mut total_bytes = 0usize;
    let mut buffer = Vec::new();

    loop {
        buffer.clear();
        let bytes = reader
            .by_ref()
            .take((MAX_RECORD_BYTES + 1) as u64)
            .read_until(b'\n', &mut buffer)
            .map_err(|source| TranscriptError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        if bytes == 0 {
            break;
        }

        let line = report.records + 1;
        total_bytes = total_bytes
            .checked_add(bytes)
            .ok_or(TranscriptError::LimitExceeded {
                line,
                message: "total byte count overflowed",
            })?;
        if bytes > MAX_RECORD_BYTES {
            return Err(TranscriptError::LimitExceeded {
                line,
                message: "record exceeds 4 MiB",
            });
        }
        if total_bytes > MAX_TOTAL_BYTES {
            return Err(TranscriptError::LimitExceeded {
                line,
                message: "file exceeds 64 MiB",
            });
        }
        if report.records >= MAX_RECORDS {
            return Err(TranscriptError::LimitExceeded {
                line,
                message: "file exceeds 100000 records",
            });
        }

        while matches!(buffer.last(), Some(b'\n' | b'\r')) {
            buffer.pop();
        }
        if buffer.is_empty() {
            return Err(TranscriptError::InvalidRecord {
                line,
                message: "record is empty",
            });
        }

        let envelope: Envelope =
            serde_json::from_slice(&buffer).map_err(|_| TranscriptError::InvalidRecord {
                line,
                message: "record must be a valid versioned JSON object",
            })?;
        if envelope.schema_version != 1 {
            return Err(TranscriptError::InvalidRecord {
                line,
                message: "schema_version must be 1",
            });
        }
        if envelope.payload.len() > MAX_PAYLOAD_BYTES {
            return Err(TranscriptError::LimitExceeded {
                line,
                message: "payload exceeds 1 MiB",
            });
        }

        report.records += 1;
        if !diagnostics_saturated {
            inspect_payload(&envelope, line, &mut state, &mut report.findings);
            if report.findings.len() >= MAX_FINDINGS {
                report.findings.truncate(MAX_FINDINGS - 1);
                report.findings.push(TranscriptFinding {
                    code: TranscriptFindingCode::TranscriptDiagnosticsTruncated,
                    severity: TranscriptSeverity::Warning,
                    line,
                    direction: envelope.direction,
                    message: "diagnostics stopped after 10000 findings",
                });
                diagnostics_saturated = true;
            }
        }
    }

    Ok(report)
}

fn inspect_payload(
    envelope: &Envelope,
    line: usize,
    state: &mut LifecycleState,
    findings: &mut Vec<TranscriptFinding>,
) {
    if envelope.payload.contains(['\n', '\r']) {
        findings.push(finding(
            TranscriptFindingCode::TranscriptMultilineMessage,
            line,
            envelope.direction,
            "stdio messages must not contain embedded newline characters",
        ));
        return;
    }

    let value: Value = match serde_json::from_str(&envelope.payload) {
        Ok(value) => value,
        Err(_) => {
            let (code, message) = match envelope.direction {
                TranscriptDirection::ServerToClient => (
                    TranscriptFindingCode::TranscriptStdoutPollution,
                    "server stdout contains a non-JSON MCP message",
                ),
                TranscriptDirection::ClientToServer => (
                    TranscriptFindingCode::TranscriptInvalidJson,
                    "client stdio payload is not valid JSON",
                ),
            };
            findings.push(finding(code, line, envelope.direction, message));
            return;
        }
    };

    let Some(message) = value.as_object() else {
        findings.push(finding(
            TranscriptFindingCode::TranscriptInvalidJsonrpcMessage,
            line,
            envelope.direction,
            "MCP stdio payload must be a JSON-RPC object",
        ));
        return;
    };
    if message.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        findings.push(finding(
            TranscriptFindingCode::TranscriptInvalidJsonrpcVersion,
            line,
            envelope.direction,
            "MCP messages must declare jsonrpc 2.0",
        ));
        return;
    }

    let method = message.get("method").and_then(Value::as_str);
    let has_result = message.contains_key("result");
    let has_error = message.contains_key("error");
    let raw_id = message.get("id");
    let id = raw_id.and_then(normalize_id);
    let params_are_valid = message
        .get("params")
        .is_none_or(|params| params.is_object() || params.is_array());
    let error_is_valid = message.get("error").is_none_or(|error| {
        error.as_object().is_some_and(|error| {
            error.get("code").and_then(Value::as_i64).is_some()
                && error.get("message").and_then(Value::as_str).is_some()
        })
    });
    let valid_method = method.is_some()
        && !has_result
        && !has_error
        && params_are_valid
        && (raw_id.is_none() || id.is_some());
    let valid_response =
        method.is_none() && id.is_some() && has_result != has_error && error_is_valid;
    if !valid_method && !valid_response {
        findings.push(finding(
            TranscriptFindingCode::TranscriptInvalidJsonrpcMessage,
            line,
            envelope.direction,
            "payload is not a valid JSON-RPC request, notification, or response",
        ));
        return;
    }

    if method == Some("initialize") {
        if !matches!(envelope.direction, TranscriptDirection::ClientToServer) {
            findings.push(finding(
                TranscriptFindingCode::TranscriptInitializeOrder,
                line,
                envelope.direction,
                "initialize must be sent from client to server",
            ));
            return;
        }
        if state.initialize_seen {
            findings.push(finding(
                TranscriptFindingCode::TranscriptDuplicateInitialize,
                line,
                envelope.direction,
                "a session must not contain more than one initialize request",
            ));
            return;
        }
        if line != 1 {
            findings.push(finding(
                TranscriptFindingCode::TranscriptInitializeOrder,
                line,
                envelope.direction,
                "initialize must be the first interaction in a captured session",
            ));
        }
        state.initialize_seen = true;
        state.initialize_id = id;
        if state.initialize_id.is_none() {
            findings.push(finding(
                TranscriptFindingCode::TranscriptInvalidJsonrpcMessage,
                line,
                envelope.direction,
                "initialize must be a request with a string or integer id",
            ));
        }
        return;
    }

    if method == Some("notifications/initialized") {
        if !matches!(envelope.direction, TranscriptDirection::ClientToServer) || id.is_some() {
            findings.push(finding(
                TranscriptFindingCode::TranscriptInitializeOrder,
                line,
                envelope.direction,
                "initialized must be a client-to-server notification without an id",
            ));
            return;
        }
        if !state.initialize_seen || !state.initialize_response_seen {
            findings.push(finding(
                TranscriptFindingCode::TranscriptInitializeOrder,
                line,
                envelope.direction,
                "initialized notification appears before the initialize response",
            ));
        }
        return;
    }

    if matches!(envelope.direction, TranscriptDirection::ServerToClient)
        && method.is_none()
        && id == state.initialize_id
        && has_result
    {
        state.initialize_response_seen = true;
    }
}

fn normalize_id(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(format!("s:{value}")),
        Value::Number(value) if value.is_i64() || value.is_u64() => Some(format!("n:{value}")),
        _ => None,
    }
}

fn finding(
    code: TranscriptFindingCode,
    line: usize,
    direction: TranscriptDirection,
    message: &'static str,
) -> TranscriptFinding {
    TranscriptFinding {
        code,
        severity: TranscriptSeverity::Error,
        line,
        direction,
        message,
    }
}

fn serialize_path<S>(path: &Path, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&path.to_string_lossy())
}
