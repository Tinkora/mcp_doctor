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

> Status: Alpha (`v0.1.12` scope). This release is intentionally CLI-only and
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
  startup failures; [#11671](https://github.com/cline/cline/issues/11671)
  reports that the documented CLI path differs from the file the CLI reads.
- [Continue #4791](https://github.com/continuedev/continue/issues/4791) and
  [#7509](https://github.com/continuedev/continue/issues/7509) report missing
  `npx` and timeouts.
- [GitHub MCP server #1396](https://github.com/github/github-mcp-server/issues/1396)
  reports local server startup configuration problems.
- [GitHub Copilot CLI #3379](https://github.com/github/copilot-cli/issues/3379)
  reports a repository definition silently shadowing a same-named user server;
  [#4478](https://github.com/github/copilot-cli/issues/4478) reports case-only
  collisions starting duplicate MCP processes.
- Claude Code [#54803](https://github.com/anthropics/claude-code/issues/54803)
  reports user-scope configuration being written where listing did not read it,
  while [#77325](https://github.com/anthropics/claude-code/issues/77325) and
  [#58919](https://github.com/anthropics/claude-code/issues/58919) show invalid
  and nested entries in `~/.claude.json` disrupting MCP configuration.
- Codex [#37616](https://github.com/openai/codex/issues/37616) and
  [#13464](https://github.com/openai/codex/issues/13464) show malformed or
  duplicate MCP tables making `config.toml` unusable, while
  [#26011](https://github.com/openai/codex/issues/26011) and
  [#33104](https://github.com/openai/codex/issues/33104) report stale paths and
  missing MCP commands. [#30125](https://github.com/openai/codex/issues/30125)
  shows a remote server appearing authenticated even when its configured
  bearer-token environment variable is absent from the client process.
  [#35448](https://github.com/openai/codex/issues/35448) shows disabled plugin
  entries remaining discoverable to third-party MCP tools.
  [#22842](https://github.com/openai/codex/issues/22842) reports plugin-root
  relative paths that fail when a client resolves them from another working
  directory.

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
workspace and the current user's known Codex, Claude Code, Claude Desktop,
Cline, Cursor, VS Code, and GitHub Copilot CLI paths. Repository discovery
includes `.codex/config.toml`, `.devcontainer/devcontainer.json`,
`.vscode/mcp.json`, `.mcp.json`, `.github/mcp.json`,
`.github/mcp-config.json`, and `.cursor/mcp.json`; user discovery includes
Codex's `~/.codex/config.toml`, Claude Code's `~/.claude.json`, Copilot CLI's
`~/.copilot/mcp-config.json`, and the platform's VS Code user `mcp.json`. For
the Cline CLI, it also discovers the reported
`~/.cline/data/settings/cline_mcp_settings.json` path when present. For Claude
Code, MCP Doctor inspects top-level user servers and only the local servers
belonging to the current workspace.

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

The MVP reads JSON, JSONC (JSON with comments and trailing commas), and Codex
TOML:

- a top-level `mcpServers` map (Claude Desktop and Cline-style files);
- Cline CLI's known user-level `~/.cline/data/settings/cline_mcp_settings.json`
  path when it exists; the path is included for discovery only and follows the
  implementation path documented in [Cline #11671](https://github.com/cline/cline/issues/11671);
- a top-level `servers` map (VS Code-style entries);
- a VS Code Dev Container `customizations.vscode.mcp.servers` map in
  `.devcontainer/devcontainer.json` (see the
  [official VS Code MCP documentation](https://code.visualstudio.com/docs/agent-customization/mcp-servers));
- Claude Code user servers at top-level `mcpServers` and current-workspace local
  servers under `projects[workspace].mcpServers` in `~/.claude.json`, following
  the [official Claude Code scope documentation](https://code.claude.com/docs/en/mcp#scope-hierarchy-and-precedence);
- Codex user and current-workspace servers under `[mcp_servers.<name>]` in
  `~/.codex/config.toml` and `.codex/config.toml`, following the
  [official Codex MCP documentation](https://developers.openai.com/codex/mcp/);
- stdio fields `command`, optional string-array `args`, optional string `cwd`,
  and optional string-map `env`.
- Codex `enabled = false` servers are skipped for launch checks and receive an
  informational warning because third-party MCP discovery tools may not honor
  the Codex-specific flag. `env_vars` entries may be names or
  `{ name, source = "local" | "remote" }` tables. Their structure is
  validated, but the named process values are never looked up or emitted.
- For remote Codex URL entries, `bearer_token_env_var` must be a non-empty
  string. MCP Doctor warns when that environment-variable name is absent from
  its current process without retrieving or reporting the token value or the
  configured variable name.
- When discovering known user paths, MCP Doctor inspects up to 128
  `.codex/plugins/cache/<marketplace>/<plugin>/<version>/.mcp.json` files and
  applies the same static checks. Relative command and cwd findings in those
  files explicitly warn that the Codex plugin root or client may provide the
  base; this is grounded in [Codex issue #22842](https://github.com/openai/codex/issues/22842).
- JSON and JSONC files may begin with a UTF-8 BOM; MCP Doctor removes it before
  parsing, matching common Windows editor output.
- VS Code `${input:name}` references are client-provided inputs, not unresolved
  process-environment placeholders. They are never expanded.
- Exact and case-only duplicate stdio server names across inspected entries are
  reported without applying a client-specific precedence rule.

Remote entries (`url`, HTTP, SSE, or another non-stdio `type`) are still
reported as unsupported and are not contacted. The Codex bearer-token check is
only an environment preflight; it does not validate authentication or protocol
behavior. Plugin manifests and lifecycle settings are not resolved; discovered
plugin-cache `.mcp.json` files are inspected as standalone configurations only.
Bounded plugin-cache discovery is read-only. YAML, MCP catalog files, protocol
handshakes, and server execution are out of scope until independent demand and
compatibility evidence justify them.

## Safety and privacy

MCP Doctor is read-only by default. It reads the selected JSON, JSONC, or Codex
TOML file and file metadata, reads the process `PATH` to test bare command
discoverability, and retains process environment names only for the Codex
remote-auth presence check. It does not spawn a process, perform a network
request, retrieve a matching environment value, or include configured
environment values or bearer-token variable names in its reports. Placeholder
diagnostics are generic and do not echo the placeholder token. VS Code
`${input:name}` references are exempt because their values are provided by the
client, not read from the process environment. Codex `env_vars` values are not
read. Other environment keys and server names can appear as locations, while
terminal control characters in human output are escaped. When reading
`~/.claude.json`, project entries other than the current workspace are ignored.
Remove secrets before sharing a config file.

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
output, Claude Code user/local scope selection, unsupported transports,
Codex TOML parsing and discovery, malformed input, JSON path encoding, CLI exit
codes, and the no-execution boundary.

Read the [product specification](docs/PRODUCT_SPEC.md) for the evidence gate,
supported discovery paths, and stop conditions. See [CONTRIBUTING.md](CONTRIBUTING.md),
[SECURITY.md](SECURITY.md), and [CHANGELOG.md](CHANGELOG.md) before making a
change.

## License

[MIT](LICENSE) Copyright Tinkora contributors.
