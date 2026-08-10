# Security and Privacy

[English](SECURITY.md) | [简体中文](SECURITY.zh-CN.md)

MCP Doctor reads selected JSON files and filesystem metadata. It reads the
process `PATH` for bare-command lookup and reads the home-directory location
only to discover conventional files. Although the selected JSON is parsed in
memory, configured environment values are never emitted, and matching values
are never retrieved from the process environment. The tool does not execute a
configured command or make a network request.

Do not paste secrets into public issues. Report a suspected vulnerability using
GitHub's private vulnerability reporting for the repository when it is enabled.
Include a minimal reproduction that does not contain credentials.
