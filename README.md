# ntkn (นับโทเค็น)

[![version](https://img.shields.io/badge/version-0.11.4-blue)](https://github.com/dhanabhon/ntkn/blob/main/CHANGELOG.md)

`ntkn` (pronounced "nub-token" 🇹🇭) is project-level token accounting for AI
agent work. It shows how many tokens each project uses, split by provider and
model, so you can see where AI usage is going instead of guessing from scattered
tool output.

Each project gets its own local SQLite ledger. Hooks call `ntkn record` after an
AI turn, and `ntkn usage` / `ntkn stats` turn those rows into per-project totals,
activity, and model-level breakdowns.

> [!WARNING]
> This project is currently a **work in progress** and is **not ready for general or production use**. Features may be incomplete, unstable, or subject to breaking changes.

## Why project-level token accounting?

AI tools usually show token usage inside their own session, if they show it at
all. That makes it hard to answer simple project questions:

- Which project is using the most tokens?
- Which model or provider is responsible for that usage?
- How much activity happened in the last week or month?
- Did a reset, hook, or agent switch change the numbers?

ntkn keeps that accounting beside the codebase. The result is a small local
ledger per project, with no remote service and no shared global database.

## How it works

```text
=======================================================================
                   NTKN WORKFLOW ARCHITECTURE
=======================================================================

[ Your Project Folder ]
 |-- code_files...
 |-- .ntkn/                         <-- created by `ntkn init`
 |   |-- rules/
 |   |   `-- ntkn-rules.md           <-- project id, budget, and token rules
 |   |-- hooks/
 |   |   |-- claude-code/
 |   |   |   `-- ntkn-record.sh
 |   |   `-- codex/
 |   |       `-- ntkn-record.sh
 |   `-- ntkn.sqlite                 <-- local token database for this project
 |-- .claude/settings.json           <-- Claude Code hook wiring
 |-- .cursor/hooks.json              <-- Cursor hook wiring
 `-- .agy/hooks.json                 <-- Antigravity hook wiring


THE EXECUTION LOOP

  [ User ]
      |
      | 1. Start an AI agent chat as usual.
      v
+---------------------------+
| AI Agent CLI              | 2. The tool runs with this project context.
| Claude / Codex / Cursor   |
| Antigravity (`agy`)       |
+------------+--------------+
             |
             | 3. The tool sends the prompt to the selected provider.
             v
      AI Provider
      Claude / OpenAI / Gemini / Local model
             |
             | 4. The provider returns a response and usage metadata.
             v
+---------------------------+
| AI Agent CLI              | 5. The answer is shown to the user.
+------------+--------------+
             |
             | 6. Background Stop/stop hook runs after the turn.
             |    The hook reads token usage from a payload or transcript.
             |
             |    Example:
             |    ntkn record --provider agy --model <name> --prompt <P> --comp <C>
             v
+---------------------------+
| ntkn (Rust CLI)           | 7. ntkn writes the usage row to `.ntkn/ntkn.sqlite`.
+---------------------------+

=======================================================================
```

Everything stays in the project directory. ntkn does not send token usage to a
remote service.

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
.agy/
  hooks.json
  hooks/
    ntkn-record.sh
.opencode/
  plugins/
    ntkn.js
.codex/
  hooks.json
```

The SQLite database stores one row per call. The rules file stores the
`project_id` used by `usage` and `history`. The hook files let Claude Code,
Codex, Cursor, Antigravity, and OpenCode record usage after each turn when their
hook payloads or plugin events include enough token data.

## Supported tools

| Tool | Provider | Hook event | Wiring | Automatic recording | Manual fallback |
| --- | --- | --- | --- | --- | --- |
| Claude Code | Anthropic | Stop | `.claude/settings.json` → `.ntkn/hooks/claude-code/ntkn-record.sh` | Yes, after `ntkn init` | `ntkn sync-claude` |
| Codex | OpenAI | Stop | `~/.codex/hooks.json` → `~/.codex/hooks/ntkn-dispatch.sh` → `.ntkn/hooks/codex/ntkn-record.sh` | After Terminal CLI hook trust (Desktop has no trust UI) | `ntkn sync-codex` |
| Cursor | Multi-provider | stop | `.cursor/hooks.json` → `.cursor/hooks/ntkn-record.sh` | Yes, from stop `input_tokens`/`output_tokens` | `ntkn sync-cursor` |
| Antigravity | Google / Multi-provider | stop | `.agy/hooks.json` → `.agy/hooks/ntkn-record.sh` | Yes, from stop `input_tokens`/`output_tokens` | `ntkn sync-agy` |
| OpenCode | Multi-provider | session.idle | `.opencode/plugins/ntkn.js` → `.ntkn/hooks/opencode/ntkn-record.sh` | Yes, when plugin event includes usage metadata | `ntkn sync-opencode` |

Model names are not unique across tools: `gpt-5.4` in Codex (OpenAI) and the same
slug in Cursor or Antigravity (multi-provider routing) are separate usage
streams. `ntkn usage` groups by provider and model so those streams stay
separate.

Claude Code reads session transcripts and deduplicates assistant messages; use
`ntkn sync-claude` to replay the latest transcript if totals look stale.
Codex reads `token_count` events from session JSONL; use `ntkn sync-codex` when
Stop hooks are untrusted or stale. Cursor reads `input_tokens`/`output_tokens`
from the stop hook payload; use `ntkn sync-cursor` to replay the last capture.
Antigravity uses the same stop-payload pattern with provider `agy`; use
`ntkn sync-agy` to replay the last capture. OpenCode uses a local project plugin
with provider `opencode`; use `ntkn sync-opencode` to replay the last captured
session event.

`Stop` / `stop` is the agent lifecycle event that fires after an AI turn
finishes. ntkn records usage there because responses, transcripts, and token
events are complete enough to read. The capitalization is tool-specific:
Claude Code and Codex use `Stop`; Cursor and Antigravity use `stop`; OpenCode
uses the plugin `session.idle` event. These do not stop the agent; they mean
"run this hook after the agent stops responding for this turn."

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
| `ntkn -h`, `ntkn --help` | Print the same splash output as `ntkn` |
| `ntkn -V`, `ntkn --version` | Print version |
| `ntkn init --project <NAME>` | Create `.ntkn/`, hooks, and rules for the current directory |
| `ntkn record --project <PROJ> --provider <TOOL> --model <MODEL> --prompt <NUM> --comp <NUM>` | Append one usage row |
| `ntkn usage` | Show totals grouped by provider and model |
| `ntkn status` | Show project setup and hook health |
| `ntkn stats` | Show a green activity heatmap and usage summary |
| `ntkn history --limit <NUM>` | Show recent rows (default: `10`) |
| `ntkn reset` | Delete usage rows for the current project (prompts for confirmation) |
| `ntkn clean` | Reset hook sync state files without deleting usage rows (prompts for confirmation) |
| `ntkn sync-claude` | Pull Claude Code usage from the latest transcript for this project |
| `ntkn sync-codex` | Pull Codex usage from the latest session JSONL for this project |
| `ntkn sync-cursor` | Replay the last captured Cursor stop payload for this project |
| `ntkn sync-agy` | Replay the last captured Antigravity stop payload for this project |
| `ntkn sync-opencode` | Replay the last captured OpenCode plugin event for this project |

### Examples

Print the version:

```sh
ntkn -V
ntkn --version
```

Print the splash and command guide:

```sh
ntkn -h
ntkn --help
```

Initialize a project:

```sh
ntkn init --project my-project
```

Record a call manually:

```sh
ntkn record --project my-project --provider manual --model gpt-5 --prompt 1200 --comp 300
```

Show totals for the current project:

```sh
ntkn usage
```

Check setup and hook health:

```sh
ntkn status
```

Show activity stats:

```sh
ntkn stats
```

Show recent rows:

```sh
ntkn history --limit 20
```

Reset usage stats for the current project:

```sh
ntkn reset
```

Reset hook sync state for the current project:

```sh
ntkn clean
```

Refresh Claude Code totals after a session:

```sh
ntkn sync-claude
ntkn usage
```

Refresh Codex totals after a session:

```sh
ntkn sync-codex
ntkn usage
```

Replay the last Cursor stop capture:

```sh
ntkn sync-cursor
ntkn usage
```

Replay the last Antigravity stop capture:

```sh
ntkn sync-agy
ntkn usage
```

Replay the last OpenCode plugin event:

```sh
ntkn sync-opencode
ntkn usage
```

### `record` flags

| Flag | Required | Default | Description |
| --- | --- | --- | --- |
| `--project` | yes | — | Project id from `.ntkn/rules/ntkn-rules.md` |
| `--provider` | no | `manual` | Source tool: `manual`, `claude-code`, `codex`, `cursor`, `agy`, or `opencode` |
| `--model` | yes | — | Model name for this call |
| `--prompt` | yes | — | Prompt-side token count |
| `--comp` | yes | — | Completion-side token count |

Bundled hooks set `--provider` automatically (`claude-code`, `codex`, `cursor`,
`agy`, `opencode`). For manual entries, omit `--provider` or pass
`--provider manual`.

`usage` groups usage by provider and model. It shows prompt tokens, completion
tokens, and total tokens.

`stats` shows a green activity heatmap for the last year plus all-time, last
7 days, last 30 days, favorite model, and total token summary. It uses local
SQLite timestamps only.

`sync-opencode` replays `.ntkn/opencode-last-event.json`, which is written by
the OpenCode project plugin when OpenCode emits `session.idle`. Use it after an
OpenCode session if `ntkn usage` looks stale.

`reset` asks for confirmation and deletes only usage rows for the current
`project_id`. It keeps `.ntkn/rules/ntkn-rules.md`, hook files, and the database
schema.

`clean` asks for confirmation and rewrites hook sync state files to empty valid
JSON. It does not delete usage rows. Use it when you want hooks and sync commands
to forget what they have already seen:

```text
.ntkn/codex-state.json      -> {"sessions":{}}
.ntkn/cursor-state.json     -> {"sessions":{},"seen_generations":{}}
.ntkn/agy-state.json        -> {"sessions":{},"seen_generations":{}}
.ntkn/opencode-state.json   -> {"seen":{}}
```

For a clean retest after a hook bug, run `ntkn reset`, then `ntkn clean`, then
start a fresh AI turn. Running `sync-*` after `clean` can replay older local
sessions again.

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
ntkn record --project other-project --provider manual --model gpt-5 --prompt 1200 --comp 300
ntkn usage
```

You only need `ntkn init` once per project. After changing `ntkn`, run
`cargo install --path .` again from this repo, then go back to the other project
and run `ntkn usage` or `ntkn record`. Existing `.agents/ntkn.sqlite` files are
reused.

## Uninstall

To remove ntkn from a project, delete the project-local artifacts:

```sh
rm -rf .ntkn
rm -rf .agents
rm -f .claude/settings.json
```

If a project is using Codex hooks, also remove:

```sh
rm -f .codex/hooks.json
```

If a project is using Cursor, Antigravity, or OpenCode hooks, also remove:

```sh
rm -rf .cursor
rm -rf .agy
rm -rf .opencode
```

If you installed ntkn globally and want to remove the binary:

```sh
cargo uninstall ntkn
```

If you no longer want global hook wiring, also remove:

```sh
rm -f ~/.codex/hooks/ntkn-dispatch.sh
```

Removing `.ntkn` is enough to stop local collection for that project; keep hook
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
  timestamp TEXT NOT NULL,
  timestamp_unix_ms INTEGER NOT NULL DEFAULT 0
);
```

## Hook notes

`record` exits with a clear error if `.ntkn/ntkn.sqlite` does not exist. Run
`ntkn init --project <name>` once per project before wiring the hook.

Bundled Claude Code, Codex, Cursor, Antigravity, and OpenCode hooks record token
counts.

### Claude Code

`ntkn init` installs a Claude Code Stop hook that records usage after each turn.

Layout after init:

```text
.ntkn/
  ntkn.sqlite
  claude-state.json
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
            "args": ["${CLAUDE_PROJECT_DIR}/.ntkn/hooks/claude-code/ntkn-record.sh"],
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
totals with `ntkn usage`.

If totals look stale, run:

```sh
ntkn sync-claude
```

That replays the latest Claude Code transcript for this project and preserves
the same dedupe state as the Stop hook.

### Codex

Unlike RTK (which hooks shell commands via `RTK.md`), ntkn reads Codex API usage
from session JSONL files. **Codex Desktop has no `/hooks` command**, so automatic
Stop-hook recording usually does not happen until you trust hooks from the
Terminal CLI.

**Recommended after Codex work:**

```sh
ntkn sync-codex
ntkn usage
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
`last_token_usage` block. The hook records the final token event for each new
turn since the last Stop and deduplicates by timestamp in `.ntkn/codex-state.json`.
Usage is grouped by model, so model switches within a session are tracked
separately.

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

Prompt-side counts use Codex `input_tokens`. Completion counts use Codex
visible output tokens, calculated as `output_tokens - reasoning_output_tokens`
when reasoning details are present. Cached input and reasoning details are not
added again.

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
not include usage; the stop hook is the source of truth. If reasoning output is
reported separately, completion is recorded as visible output only. Each capture
is saved to `.ntkn/cursor-last-payload.json` for `ntkn sync-cursor` replay.

**Recommended if totals look stale:**

```sh
ntkn sync-cursor
ntkn usage
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

### Antigravity

`ntkn init` installs an Antigravity project `stop` hook.

Layout after init:

```text
.agy/
  hooks.json
  hooks/
    ntkn-record.sh
.ntkn/
  agy-state.json
```

The bundled hook reads per-turn `input_tokens` and `output_tokens` from the
Antigravity stop payload. If reasoning output is reported separately,
completion is recorded as visible output only. The hook saves the last capture
to `.ntkn/agy-last-payload.json`.

**Recommended if totals look stale:**

```sh
ntkn sync-agy
ntkn usage
```

Requirements:

- `ntkn` on your PATH
- `jq` installed
- Run `ntkn init --project <name>` once in the project root

Hook wiring in `.agy/hooks.json`:

```json
{
  "version": 1,
  "hooks": {
    "stop": [
      {
        "command": ".agy/hooks/ntkn-record.sh",
        "timeout": 30
      }
    ]
  }
}
```

Manual fallback when Antigravity does not send usage fields:

```sh
ntkn record --project my-project --provider agy --model gemini-3 --prompt 1200 --comp 300
```

### OpenCode

`ntkn init` installs an OpenCode project plugin.

Layout after init:

```text
.opencode/
  plugins/
    ntkn.js
.ntkn/
  hooks/
    opencode/
      ntkn-record.sh
  opencode-state.json
```

OpenCode loads project plugins from `.opencode/plugins/` at startup. The bundled
plugin listens for `session.idle`, saves the last event to
`.ntkn/opencode-last-event.json`, and records usage when that event includes
token metadata. If reasoning output is reported separately, completion is
recorded as visible output only.

**Recommended if totals look stale:**

```sh
ntkn sync-opencode
ntkn usage
```

Requirements:

- `ntkn` on your PATH
- `jq` installed
- Restart OpenCode after running `ntkn init --project <name>`

Manual fallback when OpenCode does not send usage fields:

```sh
ntkn record --project my-project --provider opencode --model gpt-5 --prompt 1200 --comp 300
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
