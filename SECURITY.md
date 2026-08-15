# Security and Privacy

[English](SECURITY.md) | [简体中文](SECURITY.zh-CN.md)

MCP Doctor reads selected JSON, JSONC, and Codex TOML files plus filesystem
metadata. It reads the process `PATH` for bare-command lookup and reads the
home-directory location only to discover conventional files. Although selected
configuration is parsed in memory, configured environment values are used only
for static empty-value and placeholder detection. They are not emitted,
interpolated, or used as the command lookup environment. Codex `env_vars`
declarations are structurally validated, but their named values are never
retrieved from the process environment. Placeholder messages do not echo the
detected token.

Human-readable output escapes terminal control characters from paths, server
names, environment keys, and diagnostics. Ordinary Unicode text remains
readable. JSON output represents non-UTF-8 filesystem paths lossily. The tool
does not execute a configured command or make a network request.

PATH findings describe only MCP Doctor's current process. They do not prove
that a GUI client inherited the same environment. Relative working directories
are not resolved; a relative command is resolved only when an absolute `cwd`
provides a deterministic base.

Do not paste secrets into public issues. Report a suspected vulnerability using
GitHub's private vulnerability reporting for the repository when it is enabled.
Include a minimal reproduction that does not contain credentials.
