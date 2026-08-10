# Contributing

[English](CONTRIBUTING.md) | [简体中文](CONTRIBUTING.zh-CN.md)

Start with a reproducible MCP client workflow and evidence of the failure. A
proposal should identify the client/config format, the smallest user-visible
outcome, existing alternatives, and a stop condition. Keep pull requests
focused and include tests for changed behavior.

This project intentionally has no process execution or remote transport. Do not
add a new config format, server launch mode, or network feature from an
assumption alone; link independent compatibility or user evidence in the PR.

Before submitting:

```bash
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

Update both README files and the product specification when user-facing scope
changes. Use English Conventional Commits and do not commit secrets or real
MCP environment values.
