# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Codex Stop hook no longer skips recording: a missing `jq -n` flag caused the
  delta calculation to read empty stdin after the hook payload was consumed
- Codex hook now records per-turn `last_token_usage` and uses the correct
  `reasoning_output_tokens` field name
- Codex hook now catches up all missed `token_count` events since the last Stop
  and groups usage by model when the active model changes mid-session
- README documents that Codex skips untrusted hooks until you approve them in
  `/hooks`

## [0.2.0] - 2026-06-21

### Added

- Claude Code Stop hook installed by `ntkn init`
  - `.agents/hooks/claude-code/ntkn-record.sh` reads session transcripts and
    records new assistant usage per turn
  - `.claude/settings.json` template wires the hook when the file does not
    already exist
  - Session deduplication state in `.agents/ntkn-claude-state.json`
- Codex Stop hook installed by `ntkn init`
  - `.agents/hooks/codex/ntkn-record.sh` diffs cumulative `token_count` events
    from Codex session JSONL files
  - `.codex/hooks.json` template wires the hook when the file does not already
    exist
  - Session delta state in `.agents/ntkn-codex-state.json`
- Default splash screen when running `ntkn` with no subcommand
- Hook setup notes in `.agents/rules/ntkn-rules.md`
- Hook documentation in `README.md`

### Changed

- `ntkn init` now installs and refreshes hook scripts on each run
- Existing `.claude/settings.json` and `.codex/hooks.json` files are left
  unchanged so manual hook configs are preserved

## [0.1.0] - 2026-06-21

### Added

- Initial CLI for local token tracking in `.agents/ntkn.sqlite`
- Commands: `init`, `record`, `status`, and `history`
- Project rules file at `.agents/rules/ntkn-rules.md`
- Optional `--duration` field for per-call timing in milliseconds
- SQLite schema migration for older databases missing `duration_ms`

[0.2.0]: https://github.com/dhanabhon/ntkn/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/dhanabhon/ntkn/releases/tag/v0.1.0
