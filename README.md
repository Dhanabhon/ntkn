# ntkn (นับโทเค็น)

> [!WARNING]
> This project is currently a **work in progress** and is **not ready for general or production use**. Features may be incomplete, unstable, or subject to breaking changes.

`ntkn` (pronounced "nub-token" 🇹🇭) is a blazing-fast, lightweight CLI TUI written in Rust. It counts the tokens in a project folder and shows how much of each LLM's context window they would fill, helping you monitor consumption and costs when developing with AI agents on macOS.

---

## Key Features

1.  **Security Trust Gate:** On your first run in a folder, `ntkn` prompts you with a warning and allows you to trust the path using an interactive arrow-key selection menu before executing or reading local configs.
2.  **Background Watcher Daemon:** When started, `ntkn` spawns a detached background daemon to watch your project folder (using `notify` with a 100ms settle debouncer) and tracks active development time with a drift-free timer.
3.  **Real-Time TUI Dashboard:** Features a terminal UI monitor (built with `ratatui` and `crossterm`) showing total tokens, comparative matrix, occupancy gauges, and interactive confirmation modals.
4.  **TUI Command Bar:** Type `/` inside the dashboard to open the interactive Command Bar. Supports matching suggestions popover list, navigation using `Up`/`Down` arrow keys, autocompletion with `Tab`, and execution with `Enter`.
5.  **TUI Diagnostics (`/doctor`):** Run the `/doctor` command inside the Command Bar to display a centered diagnostics overlay check reporting on global configuration paths, directory trust status, background daemon PID details, local configuration validation, and API key setups.
6.  **CLI Controls:** Stop, pause, and resume the background monitor seamlessly using CLI commands:
    *   `ntkn pause`: Pauses filesystem monitoring and the active timer.
    *   `ntkn resume`: Resumes tracking and forces a catch-up scan.
    *   `ntkn stop`: Shuts down the background process and cleans up lock files.
7.  **TUI Usage Charts:** Run `ntkn stats` or `ntkn usage` to view a terminal bar chart of your historical token distribution across OpenAI, Anthropic Claude, and Google Gemini models.
8.  **Local Configuration:** Place a `.ntkn.toml` configuration in your project root to customize exclusions (`ignored_dirs`) or pin the active model (`default_model`).


---

## How It Works

*   **OpenAI GPT-4o:** Counted exactly using tiktoken's `cl100k_base` encoding.
*   **Anthropic Claude 3.5 Sonnet:** Approximated (scaled at ~0.96x of OpenAI).
*   **Google Gemini 1.5/2.0:** Approximated (scaled at ~1.02x of OpenAI).
*   **Active Model Detection:** Automatically detected via environment variables (like `AIDER_MODEL`), config files (`.aider.conf.yml`), or pinned in `.ntkn.toml`.
*   **Exclusion Matching:** Traverses folders using `ignore` (respecting your `.gitignore` and skipping binaries).

---

## Install

You need a Rust toolchain (edition 2024). Clone the repository and build:

```bash
git clone https://github.com/dhanabhon/ntkn.git
cd ntkn
cargo build --release
```

The binary will be compiled to `target/release/ntkn`.

---

## Usage

### 1. Launch TUI Dashboard
Run `ntkn` or `ntkn start` inside your project folder:
```bash
cd /path/to/your/project
ntkn
```

*   **Keyboard Shortcuts inside TUI:**
    *   `q` or `Ctrl+C`: Exit TUI dashboard (the daemon will **keep counting** in the background).
    *   `p`: Triggers a popup modal to **pause** counting.
    *   `s`: Triggers a popup modal to **stop** counting and terminate the background daemon.
    *   `/`: Activates the **Command Bar** footer.
        *   Type commands (e.g. `/start`, `/pause`, `/resume`, `/doctor`, `/stop`, `/quit`).
        *   Use `Up`/`Down` arrow keys to navigate the suggestions popup list.
        *   Press `Tab` to autocomplete.
        *   Press `Enter` to run the command.
        *   Press `Esc` to cancel and return to Normal mode.

### 2. Manage the Background Daemon
You can control the background watcher using CLI commands:
```bash
ntkn pause    # Pause monitoring & timer
ntkn resume   # Resume monitoring & timer
ntkn stop     # Stop daemon and clean up PID files
```

### 3. View Historical Statistics
Render a horizontal bar graph of historical token distributions:
```bash
ntkn stats
# or
ntkn usage
```

---

## Configuration (`.ntkn.toml`)

Create a `.ntkn.toml` in your project root folder to configure local behaviors:

```toml
[project]
ignored_dirs = ["node_modules", "target", "vendor", "dist"]
default_model = "anthropic_claude"
```

---

## Development & Testing

If you are developing or editing `ntkn` and want to test changes in other directories on your machine without compiling and installing a release binary every time, you can run cargo directly pointing to the `ntkn` source path.

1. Navigate to the project directory you want to monitor:
   ```bash
   cd /path/to/any/other/project
   ```

2. Run `ntkn` via cargo with `--manifest-path` (substitute `/Users/tom/Projects/GitHub/ntkn` with your actual local `ntkn` path):
   ```bash
   cargo run --manifest-path /Users/tom/Projects/GitHub/ntkn/Cargo.toml
   ```

3. To pass subcommands or arguments:
   ```bash
   cargo run --manifest-path /Users/tom/Projects/GitHub/ntkn/Cargo.toml -- stats
   cargo run --manifest-path /Users/tom/Projects/GitHub/ntkn/Cargo.toml -- stop
   ```

### 💡 Development Pro-Tip: Shell Helper
To avoid typing the long `--manifest-path` prefix every time, you can temporarily declare a shell function in your current terminal session:

```bash
ntkn-dev() {
    cargo run --manifest-path /Users/tom/Projects/GitHub/ntkn/Cargo.toml -- "$@"
}
```

Now you can simply run:
```bash
ntkn-dev        # Start TUI dashboard
ntkn-dev stats  # View stats bar chart
ntkn-dev stop   # Stop background daemon
```

---

## License

Distributed under the MIT License. See `LICENSE` for details.
