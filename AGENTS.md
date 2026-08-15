# Repository Guide for AI Agents

## Product boundary

MCP Doctor is a read-only CLI for static checks of local JSON/JSONC and Codex
TOML stdio MCP server configuration. It must not launch configured commands,
connect to remote transports, expand configured environment variables, or
print their values. Supported envelopes are top-level `mcpServers`, `servers`,
and Codex `mcp_servers`; add another client format only after independent
compatibility evidence is recorded in the product specification.

## Development

Use Rust 1.85 or newer and keep `Cargo.lock` committed. Run:

```bash
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

Public code comments must be written in English. Keep changes small and add a
regression test for every new diagnostic or CLI behavior.

## Commit language

Use English Conventional Commits for this repository, for example:
`feat: add static stdio config diagnostics`.
