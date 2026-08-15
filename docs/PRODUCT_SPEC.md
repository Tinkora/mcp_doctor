# MCP Doctor Product Specification

[English](PRODUCT_SPEC.md) | [简体中文](PRODUCT_SPEC.zh-CN.md)

## Evidence-backed problem

People configuring local MCP servers repeatedly hit launch failures before a
protocol request is ever made: `npx` is not on the client process `PATH`, Node
comes from an NVM shell that the host app does not inherit, or a Windows path
and working directory is wrong. These are recurring reports in the MCP server
tracker ([#40](https://github.com/modelcontextprotocol/servers/issues/40),
[#64](https://github.com/modelcontextprotocol/servers/issues/64),
[#447](https://github.com/modelcontextprotocol/servers/issues/447)), Cline
([#1948](https://github.com/cline/cline/issues/1948),
[#902](https://github.com/cline/cline/issues/902)), Continue
([#4791](https://github.com/continuedev/continue/issues/4791),
[#7509](https://github.com/continuedev/continue/issues/7509)), GitHub MCP
Server ([#1396](https://github.com/github/github-mcp-server/issues/1396)),
GitHub Copilot CLI ([#3380](https://github.com/github/copilot-cli/issues/3380),
[#4429](https://github.com/github/copilot-cli/issues/4429)), and related Stack
Overflow reports ([spawn npx](https://stackoverflow.com/questions/79534396/spawn-npx-enoent-spawn-npx-enoent-error-in-cline-vscode-mcp-server-connection),
[spawn npx/einval](https://stackoverflow.com/questions/79586881/spawn-npx-enoent-or-spawn-einval-when-configuring-mcp-server-with-cline-exte),
[VS Code WSL startup](https://stackoverflow.com/questions/79706687/unable-to-start-mcp-servers-in-vs-code-in-wsl)).

Configuration parsing is also a recurring compatibility boundary. GitHub
Copilot CLI issue [#4323](https://github.com/github/copilot-cli/issues/4323)
reports that comments in a repository `.mcp.json` cause every workspace server
to be skipped by a strict JSON parser. VS Code MCP configuration uses JSONC by
design, including comments and trailing commas.

Configuration scope collisions are another concrete failure mode. GitHub
Copilot CLI [#3379](https://github.com/github/copilot-cli/issues/3379) reports
that a repository server silently shadows a same-named user definition while
the UI displays the wrong source. [#4478](https://github.com/github/copilot-cli/issues/4478)
reports case-sensitive collision handling that starts duplicate logical servers.

Claude Code stores both user and local MCP scopes in `~/.claude.json`, with
local servers nested under the current project path. Its official
[scope documentation](https://code.claude.com/docs/en/mcp#scope-hierarchy-and-precedence)
defines that layout. Claude Code [#54803](https://github.com/anthropics/claude-code/issues/54803),
[#77325](https://github.com/anthropics/claude-code/issues/77325), and
[#58919](https://github.com/anthropics/claude-code/issues/58919) show recurring
user-scope location, invalid-entry, and nested-map failures around that file.

Codex stores user MCP servers in `~/.codex/config.toml` and project-scoped
servers in `.codex/config.toml`, as documented by the official
[Codex MCP guide](https://developers.openai.com/codex/mcp/) and
[configuration reference](https://developers.openai.com/codex/config-reference/).
Codex issues [#37616](https://github.com/openai/codex/issues/37616) and
[#13464](https://github.com/openai/codex/issues/13464) show invalid escapes and
duplicate MCP tables making the app unusable, while
[#13025](https://github.com/openai/codex/issues/13025),
[#26011](https://github.com/openai/codex/issues/26011), and
[#33104](https://github.com/openai/codex/issues/33104) show project-scope,
stale-path, and command-discovery failures.

## Smallest useful outcome

Given an explicit JSON, JSONC, or Codex TOML configuration or a small set of
conventional local configuration paths, `mcp-doctor` reports whether each local
`stdio` server is statically ready to launch. It checks command discoverability,
explicit command paths, working directories, and unresolved placeholders
without executing a command or retrieving matching values from the process
environment. When more than one file is inspected, it also reports exact and
case-only stdio server name conflicts without selecting a client-specific
winner. Human output is optimized for a developer at a terminal; JSON output
is stable enough for CI wrappers and never includes configured environment
values.

## Supported input boundary

- JSON or JSONC files with a top-level `mcpServers` map (Claude Desktop,
  Cline-style).
- JSON or JSONC files with a top-level `servers` map (VS Code-style entries with
  `type: "stdio"`).
- JSON or JSONC Dev Container files with a nested
  `customizations.vscode.mcp.servers` map, as documented by
  [VS Code](https://code.visualstudio.com/docs/agent-customization/mcp-servers).
- Claude Code's `~/.claude.json`, including top-level user `mcpServers` and the
  `projects[workspace].mcpServers` map for the current workspace. Other project
  entries are not inspected.
- Codex `config.toml` files with a top-level `mcp_servers` table. User and
  current-workspace files are discovered at `~/.codex/config.toml` and
  `.codex/config.toml`; Codex itself loads project files only for trusted
  projects.
- Server fields: `command`, `args`, optional `cwd`, and optional string `env`.
- Codex `enabled = false` entries are skipped. `env_vars` declarations are
  accepted without reading their named values from the process environment.
- Remote entries (`url`, `http`, `sse`, or another non-stdio `type`) are not
  inspected; they receive an explicit unsupported-transport diagnostic.
- Plugin-provided Codex MCP servers are ignored because their launch command is
  not present in top-level user configuration.
- YAML, catalog files, protocol handshakes, and remote transports are
  intentionally out of scope until independent compatibility evidence exists.

Automatic discovery is intentionally conservative: the current workspace
`.codex/config.toml`, `.devcontainer/devcontainer.json`, `.vscode/mcp.json`,
`.mcp.json`, `.github/mcp.json`, `.github/mcp-config.json`, and
`.cursor/mcp.json`, plus known user config locations for Codex, Claude Code,
Claude Desktop, Cline, Cursor, VS Code, and GitHub Copilot CLI on the current
platform. Codex discovery includes `~/.codex/config.toml`. Claude Code
discovery includes `~/.claude.json`; only its top-level user scope and
current-workspace local scope are selected. The Copilot paths are grounded in
repository-scoped configuration reports in
[#3380](https://github.com/github/copilot-cli/issues/3380) and
[#4429](https://github.com/github/copilot-cli/issues/4429).

## Checks and safety

- Missing or empty `command` is an error.
- Bare commands are checked against MCP Doctor's current process `PATH` without
  printing `PATH` or its entries. The result does not claim to reproduce a GUI
  client's environment or a configured `env.PATH`. Windows lookup uses the
  current `PATHEXT` with the platform defaults as a fallback.
- Absolute command paths and working directories are checked for existence and,
  on Unix, executable permission. A relative command path is checked only when
  an absolute `cwd` supplies a deterministic base. Other relative command and
  working-directory paths receive a client-context warning rather than a
  speculative error.
- Unresolved `${VAR}`, `$VAR`, `%VAR%`, or `{{VAR}}` placeholders are reported
  without echoing the token or configured value. VS Code `${input:name}`
  references are client-provided inputs and are not reported or expanded.
- Configured environment *keys* may appear in diagnostic locations. Values are
  parsed only for static empty-value and placeholder checks; they are not
  interpolated, used for command lookup, or emitted.
- Codex variables named by `env_vars` are not read from the process environment
  and are not included in findings. Disabled Codex entries produce no checks or
  cross-file name-conflict warnings.
- Claude Code project entries outside the current workspace are ignored even
  when they coexist with inspected user-scope servers in `~/.claude.json`.
- Exact and case-only duplicate stdio server names across inspected entries
  receive `server_name_conflict` warnings. The finding does not claim which
  definition a specific client version will select.
- The default command only reads files and metadata. There is no `--run` or
  implicit process spawn in this release.
- Human output escapes terminal control characters from configuration content.
  JSON paths that are not valid UTF-8 are represented lossily instead of
  causing a successful command to emit no JSON.

## CLI contract

```text
mcp-doctor [OPTIONS] [CONFIG ...]
  --format human|json     Output format (default: human)
  --ci                    Exit 1 when a check error is found; exit 2 for input errors
  --no-discover           Inspect only explicit CONFIG paths
```

With no explicit path, discovery runs. A missing discovered file is not an
error; an explicitly named missing file is an input error.

## Difference from MCP Inspector

MCP Inspector is the protocol-level debugging tool: it can launch servers,
perform handshakes, inspect tools, and work with stdio and remote transports.
MCP Doctor is a preflight layer for the common failure that happens before
those capabilities can start. It discovers client configuration, statically
checks local process prerequisites, never launches the server by default, and
offers CI-friendly JSON/exit codes. It does not replace Inspector and does not
claim protocol compatibility or server correctness.

## Success and stop conditions

The MVP is successful when a user can identify a missing `npx`/Node path, bad
working directory, malformed Codex TOML, unresolved placeholder, or cross-file
server name conflict without exposing a secret or running a server. Stop
expanding the parser when a format lacks independent compatibility evidence;
validate demand through concrete issue or discussion reports before adding
another client format or a process execution mode.
