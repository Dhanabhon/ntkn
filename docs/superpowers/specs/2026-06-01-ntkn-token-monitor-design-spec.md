# ntkn (นับ token) - CLI Real-time Token Monitor Specification

This specification describes the architectural design, directory structures, IPC models, and user interfaces for Phase 1 of `ntkn`, a real-time terminal token usage monitor built for AI-driven projects on macOS.

---

## 1. Goal Description

When developing projects using AI agents, developers need to understand their token consumption patterns and active project duration. `ntkn` runs a lightweight filesystem watcher in the background to automatically count project tokens for different LLM models in real-time, displaying comparison matrices, active times, and graphical statistics in a terminal dashboard interface.

---

## 2. Directory & Storage Structure

All persistent states and global configuration files are stored relative to the user's home directory on macOS:

*   **Global Config Directory:** `~/.config/ntkn/`
    *   **Trust Registry File:** `~/.config/ntkn/trusted_paths.toml` - List of directories trusted by the user.
    *   **State Directory:** `~/.config/ntkn/state/`
        *   `<path-hash>.json` - Active state (token counts, PID, status, elapsed timer) of a project.
        *   `<path-hash>.pid` - Daemon PID lockfile.
        *   `<path-hash>-history.json` - Historical snapshot logs of token usage for generating charts.

*   *Note:* The `<path-hash>` is the SHA-256 hash of the absolute directory path of the project.

---

## 3. Component Architecture

### A. CLI Command Router
The `ntkn` CLI routes commands based on arguments:
*   `ntkn` or `ntkn start [--ui]`: Verifies trust, launches the background daemon (if not running), and boots the real-time TUI dashboard.
*   `ntkn pause`: Requests the background daemon to pause filesystem watching and timer updates.
*   `ntkn resume`: Requests the background daemon to resume tracking.
*   `ntkn stop`: Commands the background daemon to terminate and clean up state files.
*   `ntkn stats` or `ntkn usage`: Displays a static TUI dashboard containing bar charts of historical model usage distribution.

### B. Directory Trust Gate
Before performing any scan, `ntkn` calculates the SHA-256 hash of the absolute path of the current working directory.
1. Checks if the path hash exists in `~/.config/ntkn/trusted_paths.toml`.
2. If absent, prompts the user:
   > *"Do you trust the contents of this directory? Working with untrusted contents comes with higher risk of prompt injection. Trusting the directory allows project-local config, hooks, and exec policies to load."*
   > *   `[Yes, continue]` or `[No, quit]`
3. If the user selects `Yes`, the path hash is added to `trusted_paths.toml`. If `No`, the program terminates immediately.

### C. Local Configuration Parser
Once a directory is trusted, `ntkn` scans the project root for a local `.ntkn.toml` configuration file:
*   Supports overriding exclusion paths (e.g. `ignored_dirs = ["node_modules", "target", "vendor"]`).
*   Supports pinning the active model if automatic detection fails (e.g. `default_model = "anthropic_claude"`).

### D. Background Watcher Daemon (Detached Process)
*   **Daemon Spawn:** Spawns a detached process running `ntkn daemon --watch <absolute-path>`.
*   **State Tracking:**
    *   **File Watching:** Uses the `notify` crate to watch for file writes, modifications, creations, and deletions. On any event, it triggers `scanner::ProjectScanner::scan_project` and updates counts.
    *   **Active Timer:** Tracks time elapsed in seconds. A background tick increases the counter by `1` every second.
*   **State Sync:** Writes state updates directly to `~/.config/ntkn/state/<path-hash>.json` upon file edits, and every second for the timer.

### E. Model Auto-Detection
Detects the active model using a prioritized fallback loop:
1.  **Environment Variables:** Reads standard LLM key variables (e.g., `AIDER_MODEL`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`).
2.  **Config Files:** Searches for local agent config files (e.g. `.aider.conf.yml`, `.cursorrules`, `ai.json`).
3.  **Local Project Config:** Resolves the model specified via `default_model` in the local `.ntkn.toml` file.
4.  **CLI Parameter:** Checks for options passed to the run command, e.g., `--model claude`.
5.  **Fallback:** Defaults to `"Unrecognized / Unknown"` and shows the comparison matrix.

---

## 4. User Interface Designs

### A. TUI Monitor Dashboard
The dashboard uses a layout built with `ratatui` constraints:
1.  **Header:** Shows the current directory name, active LLM model being monitored, and the running clock/timer.
2.  **Model Comparison Grid:**
    *   Rows comparing OpenAI (GPT-4o), Anthropic (Claude 3.5 Sonnet), and Google (Gemini 1.5/2.0).
    *   Columns: Token count, Max context window, occupancy percentage.
    *   Highlights the active model in bold/colored border.
3.  **Active Dialog / Confirmations:**
    *   Pressing `p` opens a centered modal prompt: `"Are you sure you want to pause? (y/n)"`.
    *   Pressing `s` opens a centered modal prompt: `"Are you sure you want to stop? (y/n)"`.
4.  **Footer Status:**
    *   Displays short guides: `[p] Pause | [s] Stop | [q] Close View (keeps counting in background)`.
    *   When the user exits via `q`, prints:
        > *"ntkn is still counting in the background. To pause, run 'ntkn pause'. To stop, run 'ntkn stop'."*

### B. TUI Usage Statistics (`ntkn stats` / `ntkn usage`)
*   Pulls historical data from `~/.config/ntkn/state/<path-hash>-history.json`.
*   Renders a `ratatui::widgets::BarChart` displaying token distributions for OpenAI, Claude, and Gemini across time bins (hourly or daily).
*   Enables checking usage history without running the real-time file watcher.

---

## 5. Verification & Testing Strategy

*   **Automated Tests:**
    *   **Tokenizer Accuracy:** Tests in `src/counter.rs` to verify token calculations on static English and Thai text passages.
    *   **Trust Registry Handling:** Unit tests validating correct writes to `trusted_paths.toml`.
*   **Manual Verification:**
    *   Deploy `ntkn` in dummy codebases, write code mockups, and observe real-time token increments and clock ticks.
    *   Verify daemon detach by starting `ntkn`, quitting TUI (`q`), modifying a file, and reopening `ntkn` to assert that token counts updated correctly during the TUI downtime.
