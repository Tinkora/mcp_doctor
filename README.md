# MCP Doctor

[English](README.md) | [简体中文](README.zh-CN.md)

<!-- markdownlint-disable MD033 -->
<p align="center">
  <a href="https://ko-fi.com/tinkora" target="_blank" rel="noopener noreferrer">
    <img
      src="https://ko-fi.com/img/githubbutton_sm.svg"
      alt="Support Tinkora on Ko-fi"
      width="520"
    >
  </a>
</p>
<!-- markdownlint-enable MD033 -->

`mcp-doctor` is a local, static preflight checker for stdio MCP server
configuration. It helps an agent developer find the launch failures that often
happen before MCP Inspector or a client can start a server: a missing `npx` or
Node binary on `PATH`, an invalid working directory, and unresolved environment
placeholders.

> Status: Alpha (`v0.1.3` scope). This release is intentionally CLI-only and
> does not launch configured commands or connect to any MCP server.

## Why this exists

The same startup failures recur in real client reports:

- [MCP servers #40](https://github.com/modelcontextprotocol/servers/issues/40)
  and [#64](https://github.com/modelcontextprotocol/servers/issues/64) cover
  Windows `npx` and Node/NVM path problems.
- [MCP servers #447](https://github.com/modelcontextprotocol/servers/issues/447)
  covers Windows path handling.
- [Cline #1948](https://github.com/cline/cline/issues/1948) and
  [#902](https://github.com/cline/cline/issues/902) report `spawn npx` and
  startup failures.
- [Continue #4791](https://github.com/continuedev/continue/issues/4791) and
  [#7509](https://github.com/continuedev/continue/issues/7509) report missing
  `npx` and timeouts.
- [GitHub MCP server #1396](https://github.com/github/github-mcp-server/issues/1396)
  reports local server startup configuration problems.
- [GitHub Copilot CLI #3379](https://github.com/github/copilot-cli/issues/3379)
  reports a repository definition silently shadowing a same-named user server;
  [#4478](https://github.com/github/copilot-cli/issues/4478) reports case-only
  collisions starting duplicate MCP processes.

The related [Stack Overflow `spawn npx` report](https://stackoverflow.com/questions/79534396/spawn-npx-enoent-spawn-npx-enoent-error-in-cline-vscode-mcp-server-connection)
shows the same failure mode outside one specific client.

## Install

Build from source with Rust 1.85 or newer:

```bash
git clone https://github.com/Tinkora/mcp_doctor.git
cd mcp_doctor
cargo install --path . --locked
```

Tagged releases also provide prebuilt archives for Linux x86-64, macOS Apple
Silicon and x86-64, and Windows x86-64. Each archive has a matching SHA-256
checksum. Releases also include a CycloneDX 1.5 dependency SBOM, with build
provenance and SBOM attestations on the
[Releases page](https://github.com/Tinkora/mcp_doctor/releases).

## Quick start

Inspect one file explicitly:

```bash
mcp-doctor ~/.cursor/mcp.json
```

With no path, MCP Doctor checks existing conventional files in the current
workspace and the current user's known Claude Desktop, Cline, Cursor, VS Code,
and GitHub Copilot CLI paths. Repository discovery includes `.vscode/mcp.json`,
`.mcp.json`, `.github/mcp.json`, `.github/mcp-config.json`, and
`.cursor/mcp.json`; user discovery includes Copilot CLI's
`~/.copilot/mcp-config.json` and the platform's VS Code user `mcp.json`.

```bash
mcp-doctor
```

For automation, use stable JSON and fail only when a static check is an error:

```bash
mcp-doctor --format json --ci .vscode/mcp.json
echo "$?" # 0 = no errors, 1 = check error, 2 = input error
```

The default human report identifies the server, location, finding code, and a
short remediation hint. It never prints configured environment values.

When multiple files are inspected, exact or case-only duplicate stdio server
names receive a `server_name_conflict` warning in each affected file. MCP
Doctor does not choose a winner because precedence differs across clients and
versions.

### Interpreting path checks

Bare commands are checked only against the environment that launched MCP
Doctor. A `path_context` warning means the command exists there, but a GUI MCP
client may inherit a different `PATH`. Values configured in `env.PATH` are not
used for lookup because configured environment values stay private. On Windows,
lookup honors the current `PATHEXT`, falling back to
`.COM;.EXE;.BAT;.CMD`.

Absolute command paths and absolute working directories can be checked
deterministically. A relative `cwd` or command path receives a warning when its
base depends on the client. If `cwd` is absolute, a relative command path is
checked against it. These warnings avoid pretending that Claude Desktop, Cline,
Cursor, and VS Code resolve every relative path the same way.

## Supported configuration

The MVP reads JSON and JSONC (JSON with comments and trailing commas):

- a top-level `mcpServers` map (Claude Desktop and Cline-style files);
- a top-level `servers` map (VS Code-style entries);
- stdio fields `command`, optional string-array `args`, optional string `cwd`,
  and optional string-map `env`.
- VS Code `${input:name}` references are client-provided inputs, not unresolved
  process-environment placeholders. They are never expanded.
- Exact and case-only duplicate stdio server names across inspected entries are
  reported without applying a client-specific precedence rule.

Remote entries (`url`, HTTP, SSE, or another non-stdio `type`) are reported as
unsupported and are not contacted. YAML, TOML, MCP catalog files, protocol
handshakes, and server execution are out of scope until independent demand and
compatibility evidence justify them.

## Safety and privacy

MCP Doctor is read-only by default. It reads the selected JSON file and file
metadata, and reads the process `PATH` only to test bare command discoverability.
It does not spawn a process, perform a network request, retrieve a matching
value from the process environment, or include configured environment values in
its reports. Placeholder diagnostics are generic and do not echo the placeholder
token. VS Code `${input:name}` references are exempt because their values are
provided by the client, not read from the process environment. Environment keys
and server names can appear as locations, while terminal control characters in
human output are escaped. Remove secrets before sharing a config file.

## MCP Inspector boundary

[MCP Inspector](https://github.com/modelcontextprotocol/inspector) is the
protocol debugger: it can launch servers, perform handshakes, inspect tools,
and work with stdio and remote transports. MCP Doctor is the preflight layer
before that workflow. It discovers client config, checks local process
prerequisites statically, and provides CI-friendly JSON and exit codes. It does
not replace Inspector or claim that a server implements the MCP protocol
correctly.

## Development

```bash
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

The test suite covers supported envelopes, JSONC comments and trailing commas,
current-process PATH and `PATHEXT`, deterministic and client-dependent path
diagnostics, placeholder redaction, VS Code input references, terminal-safe
output, unsupported transports, malformed input, JSON path encoding, CLI exit
codes, and the no-execution boundary.

Read the [product specification](docs/PRODUCT_SPEC.md) for the evidence gate,
supported discovery paths, and stop conditions. See [CONTRIBUTING.md](CONTRIBUTING.md),
[SECURITY.md](SECURITY.md), and [CHANGELOG.md](CHANGELOG.md) before making a
change.

## License

[MIT](LICENSE) Copyright Tinkora contributors.
