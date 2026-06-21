# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.11.0] - 2026-06-22

### Added

- OpenCode plugin support with `ntkn sync-opencode`

## [0.10.0] - 2026-06-22

### Added

- `ntkn stats` command with a green activity heatmap and usage summary

### Changed

- Usage rows now store `timestamp_unix_ms` alongside the ISO timestamp for
  reliable daily stats

## [0.9.0] - 2026-06-22

### Added

- Antigravity (`agy`) stop-hook support with `ntkn sync-agy`

### Changed

- `ntkn status` now reports project setup and hook health; use `ntkn usage` for
  token totals

## [0.8.0] - 2026-06-22

### Added

- `ntkn sync-claude` command for replaying the latest Claude Code transcript

### Fixed

- Claude Code hook now stores rules and dedupe state under `.ntkn/` with legacy
  `.agents/` fallback

### Removed

- `usage` no longer shows `Total Time` or `Avg Speed`; duration storage remains
  for compatibility until reliable elapsed-time tracking is implemented

## [0.7.0] - 2026-06-22

### Added

- `ntkn usage` command as the clearer primary name for usage totals

### Changed

- `ntkn status` remains available as an alias for `ntkn usage`

## [0.6.0] - 2026-06-21

### Added

- `ntkn reset` command for clearing current-project usage rows after explicit
  `RESET` confirmation

## [0.5.0] - 2026-06-21

### Added

- Provider-aware usage tracking with `record --provider`
- `status` and `history` now show provider so the same model name can be tracked
  separately across Claude Code, Codex, Cursor, and manual records
- SQLite migration adds `usage.provider` with `unknown` for older rows

## [0.4.0] - 2026-06-21

### Added

- `ntkn sync-cursor` command replays the last captured Cursor stop payload from
  `.ntkn/cursor-last-payload.json`
- README supported-tools table adds a Provider column for Anthropic, OpenAI, and
  multi-provider Cursor routing

### Fixed

- Cursor stop hook parses native `input_tokens` and `output_tokens` fields from
  the stop payload, with dedupe state in `.ntkn/cursor-state.json`

## [0.3.1] - 2026-06-21

### Added

- Project-level `default_duration_ms` rules setting for omitted `record --duration`
  values

## [0.3.0] - 2026-06-21

### Added

- Cursor project hook support installed by `ntkn init`
  - `.cursor/hooks.json` wires the Cursor `stop` hook
  - `.cursor/hooks/ntkn-record.sh` records usage when Cursor provides token
    fields in the hook payload
- README documents Cursor setup, manual fallback recording, uninstall cleanup,
  version flags, and Codex hook trust behavior

### Fixed

- Codex Stop hook no longer skips recording: a missing `jq -n` flag caused the
  delta calculation to read empty stdin after the hook payload was consumed
- Codex hook now records per-turn `last_token_usage` and uses the correct
  `reasoning_output_tokens` field name
- Codex hook now catches up all missed `token_count` events since the last Stop
  and groups usage by model when the active model changes mid-session
- README documents that Codex skips untrusted hooks until approved from the
  Terminal CLI startup prompt (Codex Desktop has no `/hooks` command)
- `ntkn sync-codex` command pulls Codex usage from the latest session JSONL
- Project data moves to `.ntkn/` (writable in Codex sandbox) with legacy
  `.agents/` migration on init

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

[0.11.0]: https://github.com/dhanabhon/ntkn/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/dhanabhon/ntkn/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/dhanabhon/ntkn/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/dhanabhon/ntkn/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/dhanabhon/ntkn/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/dhanabhon/ntkn/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/dhanabhon/ntkn/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/dhanabhon/ntkn/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/dhanabhon/ntkn/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/dhanabhon/ntkn/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/dhanabhon/ntkn/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/dhanabhon/ntkn/releases/tag/v0.1.0
