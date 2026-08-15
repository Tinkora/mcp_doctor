# Changelog

All notable changes to MCP Doctor are documented here.

<!-- markdownlint-disable MD024 -->

## [Unreleased]

## [0.1.3] - 2026-08-15

### Added

- Report exact and case-only MCP server name conflicts across inspected stdio
  configuration entries without guessing client precedence.

## [0.1.2] - 2026-08-15

### Added

- Discover repository-level `.github/mcp.json` and `.github/mcp-config.json`
  files used by GitHub Copilot CLI alongside existing workspace configs.
- Discover GitHub Copilot CLI's `~/.copilot/mcp-config.json` and platform VS
  Code user-level `mcp.json` files without executing or contacting any server.

## [0.1.1] - 2026-08-15

### Added

- Accept JSONC comments and trailing commas used by VS Code MCP
  configurations, while keeping JSON5-only syntax outside the supported
  boundary.
- Treat VS Code `${input:name}` references as client-provided inputs instead of
  unresolved process-environment placeholders.

### Changed

- Migrated SBOM attestations from the deprecated `actions/attest-sbom`
  wrapper to `actions/attest`.
- Made the release workflow contract verify full commit-SHA action pins
  without coupling dependency updates to one obsolete action revision.

## [0.1.0] - 2026-08-11

### Added

- Static JSON diagnostics for local stdio MCP server configuration.
- Conservative workspace and user-path discovery for common client files.
- Human and JSON output with `--ci` exit codes.
- Current-process PATH diagnostics with Windows `PATHEXT` support and explicit
  warnings for client-dependent relative paths.
- Terminal-safe human output and loss-tolerant JSON path serialization.
- Tests proving configured environment values and placeholder tokens are not
  emitted and commands are never executed.
- Four-platform release archives with SHA-256 checksums, a CycloneDX 1.5 SBOM,
  build provenance, and SBOM attestations.

<!-- markdownlint-enable MD024 -->
