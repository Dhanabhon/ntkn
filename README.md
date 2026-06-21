# ntkn (นับโทเค็น)

[![version](https://img.shields.io/badge/version-0.6.0-blue)](https://github.com/dhanabhon/ntkn/blob/main/CHANGELOG.md)

`ntkn` (pronounced "nub-token" 🇹🇭) is a local token ledger for AI agent runs.
It records provider, model name, prompt tokens, completion tokens, and optional
execution time in a SQLite database inside the current project.

It is designed for hooks. Call `ntkn record` after an API request and keep the
accounting local.

> [!WARNING]
> This project is currently a **work in progress** and is **not ready for general or production use**. Features may be incomplete, unstable, or subject to breaking changes.

## What it stores

`ntkn init` creates this layout:

```text
.ntkn/
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
.cursor/
  hooks.json
  hooks/
    ntkn-record.sh
.codex/
  hooks.json
```

The SQLite database stores one row per call. The rules file stores the
`project_id` used by `status` and `history`. The hook files let Claude Code,
Codex, and Cursor record usage after each turn when their hook payloads include
enough token data.

## Supported tools

| Tool | Provider | Hook | Wiring | Automatic recording | Manual fallback |
| --- | --- | --- | --- | --- | --- |
| Claude Code | Anthropic | Stop | `.claude/settings.json` → `.ntkn/hooks/claude-code/ntkn-record.sh` | Yes, after `ntkn init` | `ntkn record` |
| Codex | OpenAI | Stop | `~/.codex/hooks.json` → `~/.codex/hooks/ntkn-dispatch.sh` → `.ntkn/hooks/codex/ntkn-record.sh` | After Terminal CLI hook trust (Desktop has no trust UI) | `ntkn sync-codex` |
| Cursor | Multi-provider | stop | `.cursor/hooks.json` → `.cursor/hooks/ntkn-record.sh` | Yes, from stop `input_tokens`/`output_tokens` | `ntkn sync-cursor` |

Model names are not unique across tools: `gpt-5.4` in Codex (OpenAI) and the same
slug in Cursor (multi-provider routing) are separate usage streams. `ntkn status`
groups by provider and model so those streams stay separate.

Claude Code reads session transcripts and deduplicates assistant messages.
Codex reads `token_count` events from session JSONL; use `ntkn sync-codex` when
Stop hooks are untrusted or stale. Cursor reads `input_tokens`/`output_tokens`
from the stop hook payload; use `ntkn sync-cursor` to replay the last capture.

See [Hook notes](#hook-notes) for setup details per tool.

## Build

```sh
cargo build --release
```

The binary is written to `target/release/ntkn`.

## Usage

Run `ntkn` without arguments to print the splash screen, current version, usage
examples, and local data paths:

```sh
ntkn
```

### Commands

| Command | Description |
| --- | --- |
| `ntkn` | Print splash, version, command list, and local data paths |
| `ntkn -V`, `ntkn --version` | Print version |
| `ntkn init --project <NAME>` | Create `.ntkn/`, hooks, and rules for the current directory |
| `ntkn record --project <PROJ> --provider <TOOL> --model <MODEL> --prompt <NUM> --comp <NUM> [--duration <MS>]` | Append one usage row |
| `ntkn status` | Show totals grouped by provider and model |
| `ntkn history --limit <NUM>` | Show recent rows (default: `10`) |
| `ntkn reset` | Delete usage rows for the current project (prompts for confirmation) |
| `ntkn sync-codex` | Pull Codex usage from the latest session JSONL for this project |
| `ntkn sync-cursor` | Replay the last captured Cursor stop payload for this project |

### Examples

Print the version:

```sh
ntkn -V
ntkn --version
```

Initialize a project:

```sh
ntkn init --project my-project
```

Record a call manually:

```sh
ntkn record --project my-project --provider manual --model gpt-5 --prompt 1200 --comp 300 --duration 5400
```

Show totals for the current project:

```sh
ntkn status
```

Show recent rows:

```sh
ntkn history --limit 20
```

Reset usage stats for the current project:

```sh
ntkn reset
```

Refresh Codex totals after a session:

```sh
ntkn sync-codex
ntkn status
```

Replay the last Cursor stop capture:

```sh
ntkn sync-cursor
ntkn status
```

### `record` flags

| Flag | Required | Default | Description |
| --- | --- | --- | --- |
| `--project` | yes | — | Project id from `.ntkn/rules/ntkn-rules.md` |
| `--provider` | no | `manual` | Source tool: `manual`, `claude-code`, `codex`, or `cursor` |
| `--model` | yes | — | Model name for this call |
| `--prompt` | yes | — | Prompt-side token count |
| `--comp` | yes | — | Completion-side token count |
| `--duration` | no | `default_duration_ms` from rules | Duration in milliseconds |

Bundled hooks set `--provider` automatically (`claude-code`, `codex`, `cursor`).
For manual entries, omit `--provider` or pass `--provider manual`.

`--duration` uses `default_duration_ms` from `.ntkn/rules/ntkn-rules.md` when
omitted. `ntkn init` creates this default:

```yaml
default_duration_ms: 0
```

Change it once per project if you want omitted durations to use a fixed value.
For example, `default_duration_ms: 5400` records `5.4s` for calls that do not
pass `--duration`.

`status` groups usage by provider and model. It shows prompt tokens, completion
tokens, total time, and average tokens per second. If duration is `0`, speed is
shown as `-`.

`reset` asks for confirmation and deletes only usage rows for the current
`project_id`. It keeps `.ntkn/rules/ntkn-rules.md`, hook files, and the database
schema.

## Test in another project

Install the current local build first:

```sh
cd /Users/tom/Projects/GitHub/ntkn
cargo install --path .
```

Then move to the project you want to track:

```sh
cd /path/to/other/project
ntkn init --project other-project
ntkn record --project other-project --provider manual --model gpt-5 --prompt 1200 --comp 300 --duration 5400
ntkn status
```

You only need `ntkn init` once per project. After changing `ntkn`, run
`cargo install --path .` again from this repo, then go back to the other project
and run `ntkn status` or `ntkn record`. Existing `.agents/ntkn.sqlite` files are
reused.

## Uninstall

To remove ntkn from a project, delete the project-local artifacts:

```sh
rm -rf .agents
rm -f .claude/settings.json
```

If a project is using Codex hooks, also remove:

```sh
rm -f .codex/hooks.json
```

If a project is using Cursor hooks, also remove:

```sh
rm -rf .cursor
```

If you installed ntkn globally and want to remove the binary:

```sh
cargo uninstall ntkn
```

If you no longer want global hook wiring, also remove:

```sh
rm -f ~/.codex/hooks/ntkn-dispatch.sh
```

Removing `.agents` is enough to stop local collection for that project; keep hook
files if you only want to clear history.

## Schema

```sql
CREATE TABLE usage (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  project_id TEXT NOT NULL,
  provider TEXT NOT NULL DEFAULT 'unknown',
  model_name TEXT NOT NULL,
  prompt_tokens INTEGER NOT NULL,
  completion_tokens INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL DEFAULT 0,
  timestamp TEXT NOT NULL
);
```

## Hook notes

`record` exits with a clear error if `.agents/ntkn.sqlite` does not exist. Run
`ntkn init --project <name>` once per project before wiring the hook.

Bundled Claude Code, Codex, and Cursor hooks record token counts only.
`duration_ms` is stored as `0` for hook records unless your own caller passes
`--duration`.

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

Unlike RTK (which hooks shell commands via `RTK.md`), ntkn reads Codex API usage
from session JSONL files. **Codex Desktop has no `/hooks` command**, so automatic
Stop-hook recording usually does not happen until you trust hooks from the
Terminal CLI.

**Recommended after Codex work:**

```sh
ntkn sync-codex
ntkn status
```

That pulls usage from the latest Codex session for this project. No hook trust
required.

RTK does not need Codex hook approval because agents run it explicitly as a
command prefix, such as `rtk git status`. ntkn automatic recording is different:
Codex runs `~/.codex/hooks/ntkn-dispatch.sh` in the background after a turn, so
Codex requires trust before executing that hook.

In short: `rtk` is an explicit command the agent chooses to run; ntkn
auto-recording is background executable code triggered by Codex.

`ntkn init` also installs:

- Project recorder: `.ntkn/hooks/codex/ntkn-record.sh`
- Global dispatcher: `~/.codex/hooks/ntkn-dispatch.sh`
- Global wiring: `~/.codex/hooks.json` (created if missing)

Layout after init:

```text
.ntkn/
  ntkn.sqlite
  codex-state.json
  hooks/
    claude-code/
      ntkn-record.sh
    codex/
      ntkn-record.sh
  rules/
    ntkn-rules.md
~/.codex/
  hooks/
    ntkn-dispatch.sh
  hooks.json
.claude/
  settings.json
```

Codex session JSONL files emit `token_count` events with a per-turn
`last_token_usage` block. The hook records all new events since the last Stop
and deduplicates by timestamp in `.ntkn/codex-state.json`. Usage is grouped by
model, so model switches within a session are tracked separately.

Requirements:

- `ntkn` on your PATH
- `jq` installed
- Run `ntkn init --project <name>` once in the project root

**Optional automatic recording (Terminal CLI only):**

```sh
cd /path/to/project
codex
```

When **Hooks need review** appears at startup, choose **Trust all and continue**.
Codex skips untrusted hooks silently. Codex Desktop has no trust UI of its
own, but trusting once in the CLI also covers Desktop sessions.

If you still have a project `.codex/hooks.json` from an older setup, remove it
to avoid double-recording.

Hook wiring in `~/.codex/hooks.json`:

```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "/Users/you/.codex/hooks/ntkn-dispatch.sh",
            "timeout": 30,
            "statusMessage": "Recording token usage (ntkn)"
          }
        ]
      }
    ]
  }
}
```

Codex Stop hooks must print JSON on stdout. The bundled script always exits
with `{"continue":true}` so it never blocks the agent.

If `~/.codex/hooks.json` already exists, `ntkn init` leaves it unchanged and
prints a merge note. Copy the Stop block from `hooks/codex/global-hooks.json`
in this repo.

Prompt-side counts use input plus cached input tokens. Completion counts use
output plus reasoning tokens from the turn's `last_token_usage`.

### Cursor

`ntkn init` installs a Cursor project `stop` hook.

Layout after init:

```text
.cursor/
  hooks.json
  hooks/
    ntkn-record.sh
.ntkn/
  cursor-state.json
```

Cursor project hooks run from the project root. The bundled hook reads per-turn
`input_tokens` and `output_tokens` from the Cursor stop payload. Transcripts do
not include usage; the stop hook is the source of truth. Each capture is saved to
`.ntkn/cursor-last-payload.json` for `ntkn sync-cursor` replay.

**Recommended if totals look stale:**

```sh
ntkn sync-cursor
ntkn status
```

That replays `.ntkn/cursor-last-payload.json` from the last stop hook capture.
Finish at least one agent turn first so the stop hook receives token fields.

Requirements:

- `ntkn` on your PATH
- `jq` installed
- Run `ntkn init --project <name>` once in the project root

Hook wiring in `.cursor/hooks.json`:

```json
{
  "version": 1,
  "hooks": {
    "stop": [
      {
        "command": ".cursor/hooks/ntkn-record.sh",
        "timeout": 30
      }
    ]
  }
}
```

If `.cursor/hooks.json` already exists, `ntkn init` refreshes it when the ntkn
hook is already present; otherwise it prints a merge note.

Manual fallback when Cursor does not send usage fields:

```sh
ntkn record --project my-project --provider cursor --model gpt-5 --prompt 1200 --comp 300
```

## Contribute

Use the normal Rust toolchain:

```sh
cargo fmt
cargo test
cargo clippy -- -D warnings
```

Keep changes small. If you change database behavior, make old `.agents/ntkn.sqlite`
files keep working or document the migration path in the pull request.

## License

MIT. See [LICENSE](LICENSE).
