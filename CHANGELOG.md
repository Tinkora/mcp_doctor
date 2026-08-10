# Changelog

All notable changes to MCP Doctor are documented here.

## [Unreleased]

No changes yet.

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
