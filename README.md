# ntkn (นับโทเค็น)

[![version](https://img.shields.io/badge/version-0.2.0-blue)](https://github.com/dhanabhon/ntkn/blob/main/CHANGELOG.md)

`ntkn` (pronounced "nub-token" 🇹🇭) is a local token ledger for AI agent runs.
It records prompt tokens, completion tokens, model name, and optional execution
time in a SQLite database inside the current project.

It is designed for hooks. Call `ntkn record` after an API request and keep the
accounting local.

> [!WARNING]
> This project is currently a **work in progress** and is **not ready for general or production use**. Features may be incomplete, unstable, or subject to breaking changes.

## what it stores

`ntkn init` creates this layout:

```text
.agents/
  ntkn.sqlite
  hooks/
    claude-code/
      ntkn-record.sh
    codex/
      ntkn-record.sh
  rules/
    ntkn-rules.md
.claude/
  settings.json
.codex/
  hooks.json
```

The SQLite database stores one row per call. The rules file stores the
`project_id` used by `status` and `history`. The hook files let Claude Code and
Codex record usage after each turn.

## build

```sh
cargo build --release
```

The binary is written to `target/release/ntkn`.

## usage

Run `ntkn` without arguments to print the splash screen, current version, usage
examples, and local data paths:

```sh
ntkn
```

Initialize a project:

```sh
ntkn init --project my-project
```

Record a call:

```sh
ntkn record --project my-project --model gpt-5 --prompt 1200 --comp 300 --duration 5400
```

`--duration` is optional and uses milliseconds. If you omit it, `ntkn` stores
`0`.

Show totals for the current project:

```sh
ntkn status
```

`status` groups usage by model. It shows prompt tokens, completion tokens, total
tokens, total time, and average tokens per second. If duration is `0`, speed is
shown as `-`.

Show recent rows:

```sh
ntkn history --limit 20
```

## test in another project

Install the current local build first:

```sh
cd /Users/tom/Projects/GitHub/ntkn
cargo install --path .
```

Then move to the project you want to track:

```sh
cd /path/to/other/project
ntkn init --project other-project
ntkn record --project other-project --model gpt-5 --prompt 1200 --comp 300 --duration 5400
ntkn status
```

You only need `ntkn init` once per project. After changing `ntkn`, run
`cargo install --path .` again from this repo, then go back to the other project
and run `ntkn status` or `ntkn record`. Existing `.agents/ntkn.sqlite` files are
reused.

## schema

```sql
CREATE TABLE usage (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  project_id TEXT NOT NULL,
  model_name TEXT NOT NULL,
  prompt_tokens INTEGER NOT NULL,
  completion_tokens INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL DEFAULT 0,
  timestamp TEXT NOT NULL
);
```

## hook notes

`record` exits with a clear error if `.agents/ntkn.sqlite` does not exist. Run
`ntkn init --project <name>` once per project before wiring the hook.

Bundled Claude Code and Codex hooks record token counts only. `duration_ms` is
stored as `0` for hook records unless your own caller passes `--duration`.

### Claude Code

`ntkn init` installs a Claude Code Stop hook that records usage after each turn.

Layout after init:

```text
.agents/
  ntkn.sqlite
  ntkn-claude-state.json
  hooks/
    claude-code/
      ntkn-record.sh
  rules/
    ntkn-rules.md
.claude/
  settings.json
```

The hook reads Claude Code's session transcript (`transcript_path` from the
Stop hook payload), deduplicates assistant messages by `uuid`, and calls
`ntkn record` for any new usage in that turn.

Requirements:

- `ntkn` on your PATH
- `jq` installed
- Run `ntkn init --project <name>` once in the project root

Hook wiring in `.claude/settings.json`:

```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "bash",
            "args": ["${CLAUDE_PROJECT_DIR}/.agents/hooks/claude-code/ntkn-record.sh"],
            "async": true
          }
        ]
      }
    ]
  }
}
```

If `.claude/settings.json` already exists, `ntkn init` leaves it unchanged.
Merge the Stop hook block above manually, or copy from
`hooks/claude-code/settings.json` in this repo.

Prompt-side token counts include uncached input plus cache read and cache
creation tokens. Completion-side counts use `output_tokens`. Claude Code
transcript output counts can be slightly low on some builds; input and cache
counts are usually reliable.

Re-run `ntkn init` to refresh the hook script after upgrading `ntkn`. Check
totals with `ntkn status`.

### Codex

`ntkn init` installs a Codex Stop hook that records usage after each turn.

Layout after init:

```text
.agents/
  ntkn.sqlite
  ntkn-codex-state.json
  hooks/
    codex/
      ntkn-record.sh
  rules/
    ntkn-rules.md
.codex/
  hooks.json
```

Codex session JSONL files emit `token_count` events with a per-turn
`last_token_usage` block. The hook records that block after each Stop and
deduplicates by event timestamp in `.agents/ntkn-codex-state.json`.

Requirements:

- `ntkn` on your PATH
- `jq` installed
- Run `ntkn init --project <name>` once in the project root
- Trust the hook in Codex with `/hooks` after the first run

Hook wiring in `.codex/hooks.json`:

```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "bash",
            "args": [".agents/hooks/codex/ntkn-record.sh"],
            "timeout": 30
          }
        ]
      }
    ]
  }
}
```

Codex Stop hooks must print JSON on stdout. The bundled script always exits
with `{"continue":true}` so it never blocks the agent.

If `.codex/hooks.json` already exists, `ntkn init` leaves it unchanged. Merge
the Stop hook block above manually, or copy from `hooks/codex/hooks.json` in
this repo.

Prompt-side counts use input plus cached input tokens. Completion counts use
output plus reasoning tokens from the turn's `last_token_usage`.

### Cursor

Hook templates for Cursor are not bundled yet. You can still call `ntkn record`
manually or from your own hooks.

## contribute

Use the normal Rust toolchain:

```sh
cargo fmt
cargo test
cargo clippy -- -D warnings
```

Keep changes small. If you change database behavior, make old `.agents/ntkn.sqlite`
files keep working or document the migration path in the pull request.

## license

MIT. See `LICENSE`.
