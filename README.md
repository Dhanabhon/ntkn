# ntkn (นับโทเค็น)

`ntkn` (pronounced "nub-token" 🇹🇭) is a local token ledger for AI agent runs.
It records prompt tokens, completion tokens, model name, and optional execution
time in a SQLite database inside the current project.

It is designed for hooks. Call `ntkn record` after an API request and keep the
accounting local.

## what it stores

`ntkn init` creates this layout:

```text
.agents/
  ntkn.sqlite
  rules/
    ntkn-rules.md
```

The SQLite database stores one row per call. The rules file stores the
`project_id` used by `status` and `history`.

## build

```sh
cargo build --release
```

The binary is written to `target/release/ntkn`.

## usage

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
