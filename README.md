# ntkn

> [!WARNING]
> This project is currently a **work in progress** and is **not ready for general or production use**. Features may be incomplete, unstable, or subject to breaking changes.

ntkn (pronounced "nub-token" 🇹🇭) is a fast, lightweight CLI TUI written in Rust. It counts the tokens in a project folder and shows how much of each LLM's context window they would fill.

Run it inside a repo and you get a live terminal dashboard: a token count per provider and a set of gauges showing how close you are to each model's context limit.

## What it does

ntkn walks the current directory, reads every text file it can, and counts the tokens in the combined content. It then shows that total against three models:

- OpenAI GPT-4o, counted exactly with tiktoken's `cl100k_base` encoding.
- Anthropic Claude 3.5 Sonnet, estimated.
- Google Gemini 1.5/2.0, estimated.

The Claude and Gemini figures are approximations scaled from the OpenAI count (about 0.96x and 1.02x). They give you a ballpark for planning, not an exact match for each provider's own tokenizer.

File scanning uses the `ignore` crate, so it follows your `.gitignore` and skips the files you would expect it to skip. Files that aren't valid UTF-8 text are skipped.

## Install

You need a Rust toolchain (edition 2024). Clone the repo and build with Cargo:

```bash
git clone https://github.com/tomdhanabhon/ntkn.git
cd ntkn
cargo build --release
```

The binary lands in `target/release/ntkn`.

## Usage

Run it from the project you want to measure:

```bash
cd /path/to/your/project
ntkn
```

Or run straight from the source checkout:

```bash
cargo run --release
```

ntkn scans the directory you launch it from. The dashboard shows the scanned path, a token matrix table, and three context-window gauges.

Keys:

- `q` or `Ctrl+C` to quit
- `r` to rescan the directory

## Context window limits

The gauges compare your token count against these limits:

| Model | Max context |
| --- | --- |
| GPT-4o | 128,000 |
| Claude 3.5 Sonnet | 200,000 |
| Gemini 1.5/2.0 | 1,000,000 |

Occupancy turns red once you pass 80% of a model's limit.

## How it's built

The project is small and split into four modules:

- `scanner.rs` walks the directory and concatenates file contents.
- `counter.rs` runs the tiktoken tokenizer and produces the per-provider counts.
- `ui.rs` draws the ratatui dashboard.
- `main.rs` sets up the terminal, runs the event loop, and restores the terminal on exit or panic.

It uses [ratatui](https://ratatui.rs) and [crossterm](https://github.com/crossterm-rs/crossterm) for the interface, [ignore](https://docs.rs/ignore) for scanning, and [tiktoken-rs](https://docs.rs/tiktoken-rs) for tokenizing.

## Status

Early days. ntkn currently scans the working directory only and uses one tokenizer for the base count, with estimates for the other providers. Real per-provider tokenizers and a path argument are on the list.
