# ntkn Real-time Token Monitor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a real-time background token counting daemon, directory trust verification, and an interactive TUI dashboard controller in Rust using crossterm and ratatui on macOS.

**Architecture:** The CLI binary acts as both the foreground TUI/controller and a detached background watcher daemon. State is synchronized via local JSON and PID files stored in `~/.config/ntkn/`, enabling zero-IPC file-based control.

**Tech Stack:** Rust (Edition 2024), ratatui 0.30.0, crossterm 0.29.0, notify 6.1.1 (for file watching), ignore 0.4.25 (for traversing), serde/serde_json (for state storage), and clap 4.6 (for command-line parsing).

---

### Task 1: Directory Trust Gate and Configuration

**Files:**
- Create: `src/config.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write tests for trust registry file handling**

Create a test in `src/config.rs` verifying paths can be trusted, saved, and checked.
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_trust_path() {
        let temp_dir = std::env::temp_dir().join("ntkn_test_trust");
        fs::create_dir_all(&temp_dir).unwrap();
        let registry_path = temp_dir.join("trusted_paths.toml");

        assert!(!is_path_trusted(&temp_dir, &registry_path));
        trust_path(&temp_dir, &registry_path).unwrap();
        assert!(is_path_trusted(&temp_dir, &registry_path));

        fs::remove_dir_all(&temp_dir).unwrap();
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test`
Expected: Compile error because `config` module does not exist yet.

- [ ] **Step 3: Implement trust verification and configuration parsing**

Write the implementation in `src/config.rs`:
```rust
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Default)]
pub struct TrustRegistry {
    pub trusted_hashes: Vec<String>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct LocalConfig {
    pub ignored_dirs: Option<Vec<String>>,
    pub default_model: Option<String>,
}

pub fn get_path_hash(path: &Path) -> String {
    use sha2::{Sha256, Digest};
    let absolute = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut hasher = Sha256::new();
    hasher.update(absolute.to_string_lossy().as_bytes());
    format!("{:x}", hasher.finalize())
}

fn get_global_config_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".config").join("ntkn")
}

pub fn is_path_trusted(path: &Path, registry_file: &Path) -> bool {
    let hash = get_path_hash(path);
    if let Ok(content) = fs::read_to_string(registry_file) {
        if let Ok(registry) = toml::from_str::<TrustRegistry>(&content) {
            return registry.trusted_hashes.contains(&hash);
        }
    }
    false
}

pub fn trust_path(path: &Path, registry_file: &Path) -> Result<(), std::io::Error> {
    let hash = get_path_hash(path);
    let mut registry = if let Ok(content) = fs::read_to_string(registry_file) {
        toml::from_str::<TrustRegistry>(&content).unwrap_or_default()
    } else {
        TrustRegistry::default()
    };

    if !registry.trusted_hashes.contains(&hash) {
        registry.trusted_hashes.push(hash);
        if let Some(parent) = registry_file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(registry_file, toml::to_string(&registry).unwrap())?;
    }
    Ok(())
}

pub fn verify_trust_interactive(path: &Path) -> Result<bool, std::io::Error> {
    let registry_file = get_global_config_dir().join("trusted_paths.toml");
    if is_path_trusted(path, &registry_file) {
        return Ok(true);
    }

    println!("Do you trust the contents of this directory?");
    println!("Working with untrusted contents comes with higher risk of prompt injection.");
    println!("Trusting the directory allows project-local config, hooks, and exec policies to load.");
    println!("\nChoices:\n1) Yes, continue\n2) No, quit");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();
    if trimmed == "1" || trimmed.to_lowercase() == "y" || trimmed.to_lowercase() == "yes" {
        trust_path(path, &registry_file)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn load_local_config(path: &Path) -> LocalConfig {
    let config_file = path.join(".ntkn.toml");
    if config_file.exists() {
        if let Ok(content) = fs::read_to_string(config_file) {
            if let Ok(config) = toml::from_str::<LocalConfig>(&content) {
                return config;
            }
        }
    }
    LocalConfig::default()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test`
Expected: Test passes.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: add directory trust check and local config support"
```

---

### Task 2: Background Watcher Daemon Spawning

**Files:**
- Create: `src/daemon.rs`
- Modify: `src/main.rs`, `Cargo.toml`

- [ ] **Step 1: Add dependencies to Cargo.toml**

Update `Cargo.toml` dependencies with:
```toml
toml = "0.8"
sha2 = "0.10"
dirs = "5.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
notify = "6.1"
```

- [ ] **Step 2: Run build to check dependencies**

Run: `cargo check`
Expected: Builds correctly.

- [ ] **Step 3: Implement Daemon control and background spawning**

Write daemon spawning implementation in `src/daemon.rs`:
```rust
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn get_state_file_path(path: &Path) -> PathBuf {
    let hash = crate::config::get_path_hash(path);
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("ntkn")
        .join("state")
        .join(format!("{}.json", hash))
}

pub fn get_pid_file_path(path: &Path) -> PathBuf {
    let hash = crate::config::get_path_hash(path);
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("ntkn")
        .join("state")
        .join(format!("{}.pid", hash))
}

pub fn is_daemon_running(path: &Path) -> bool {
    let pid_file = get_pid_file_path(path);
    if pid_file.exists() {
        if let Ok(pid_str) = fs::read_to_string(pid_file) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                // On macOS/Unix, kill -0 checks if process exists
                let status = Command::new("kill")
                    .arg("-0")
                    .arg(pid.to_string())
                    .status();
                return status.map(|s| s.success()).unwrap_or(false);
            }
        }
    }
    false
}

pub fn spawn_daemon(path: &Path) -> Result<(), std::io::Error> {
    if is_daemon_running(path) {
        return Ok(());
    }

    let exe = std::env::current_exe()?;
    let absolute_path = fs::canonicalize(path)?;

    Command::new(exe)
        .arg("daemon")
        .arg("--watch")
        .arg(absolute_path)
        .spawn()?;

    Ok(())
}
```

- [ ] **Step 4: Commit**

```bash
git add src/daemon.rs Cargo.toml
git commit -m "feat: add daemon spawning and execution validation"
```

---

### Task 3: Filesystem Watcher and State Sync Storage

**Files:**
- Create: `src/watcher.rs`
- Modify: `src/daemon.rs`

- [ ] **Step 1: Create State definitions and watcher loop**

Write the State definitions and notify filesystem watcher loop in `src/watcher.rs`:
```rust
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serde::{Serialize, Deserialize};
use notify::{Watcher, RecursiveMode, Result as NotifyResult};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DaemonState {
    pub pid: u32,
    pub status: String, // "Running" | "Paused" | "Stopped"
    pub start_time: u64,
    pub elapsed_seconds: u64,
    pub last_updated: u64,
    pub active_model: String,
    pub model_detected: bool,
    pub openai_gpt4o: usize,
    pub anthropic_claude: usize,
    pub google_gemini: usize,
}

pub fn run_watcher_loop(watch_path: PathBuf) -> Result<(), std::io::Error> {
    let state_file = crate::daemon::get_state_file_path(&watch_path);
    let pid_file = crate::daemon::get_pid_file_path(&watch_path);

    if let Some(parent) = state_file.parent() {
        fs::create_dir_all(parent)?;
    }

    let my_pid = std::process::id();
    fs::write(&pid_file, my_pid.to_string())?;

    let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

    let mut state = DaemonState {
        pid: my_pid,
        status: "Running".to_string(),
        start_time: now_secs,
        elapsed_seconds: 0,
        last_updated: now_secs,
        active_model: "Unrecognized".to_string(),
        model_detected: false,
        openai_gpt4o: 0,
        anthropic_claude: 0,
        google_gemini: 0,
    };

    // Initial scan
    recalculate_state(&watch_path, &mut state);
    write_state(&state_file, &state)?;

    // Setup notify directory watcher
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res: NotifyResult<notify::Event>| {
        if res.is_ok() {
            let _ = tx.send(());
        }
    }).unwrap();

    watcher.watch(&watch_path, RecursiveMode::Recursive).unwrap();

    let mut last_tick = std::time::Instant::now();
    loop {
        // Wait 1 second or respond to filesystem changes
        let file_changed = rx.recv_timeout(Duration::from_millis(250)).is_ok();

        // Read command modifications made by CLI
        if let Ok(content) = fs::read_to_string(&state_file) {
            if let Ok(updated_state) = serde_json::from_str::<DaemonState>(&content) {
                if updated_state.status == "Stopped" {
                    break;
                }
                state.status = updated_state.status;
            }
        }

        if state.status == "Running" {
            let elapsed = last_tick.elapsed();
            if elapsed >= Duration::from_secs(1) {
                state.elapsed_seconds += elapsed.as_secs();
                last_tick = std::time::Instant::now();
            }

            if file_changed {
                recalculate_state(&watch_path, &mut state);
            }
            
            state.last_updated = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            write_state(&state_file, &state)?;
        } else {
            // Keep timer tick reset while paused
            last_tick = std::time::Instant::now();
        }
    }

    let _ = fs::remove_file(&pid_file);
    Ok(())
}

fn write_state(file_path: &Path, state: &DaemonState) -> Result<(), std::io::Error> {
    let content = serde_json::to_string_pretty(state)?;
    fs::write(file_path, content)
}

fn recalculate_state(path: &Path, state: &mut DaemonState) {
    let scanned_text = crate::scanner::ProjectScanner::scan_project(path);
    let counts = crate::counter::TokenCounter::calculate_all(&scanned_text);
    state.openai_gpt4o = counts.openai_gpt4o;
    state.anthropic_claude = counts.anthropic_claude;
    state.google_gemini = counts.google_gemini;

    // Detect model fallback logic
    let local_config = crate::config::load_local_config(path);
    if let Some(ref model) = local_config.default_model {
        state.active_model = model.clone();
        state.model_detected = true;
    } else if let Ok(val) = std::env::var("AIDER_MODEL") {
        state.active_model = val;
        state.model_detected = true;
    } else {
        state.active_model = "Unrecognized".to_string();
        state.model_detected = false;
    }
}
```

- [ ] **Step 2: Connect watcher execution command to daemon logic**

Modify `src/daemon.rs` to expose the entrypoint:
```rust
pub fn run_daemon(watch_path: PathBuf) -> Result<(), std::io::Error> {
    crate::watcher::run_watcher_loop(watch_path)
}
```

- [ ] **Step 3: Commit**

```bash
git add src/watcher.rs src/daemon.rs
git commit -m "feat: implement notify file watcher and daemon state updates"
```

---

### Task 4: CLI Commands (pause, resume, stop, stats)

**Files:**
- Create: `src/stats.rs`
- Modify: `src/main.rs`, `src/daemon.rs`

- [ ] **Step 1: Write helper commands for CLI state updates**

Add following control implementation to `src/daemon.rs`:
```rust
pub fn modify_daemon_status(path: &Path, new_status: &str) -> Result<(), std::io::Error> {
    let state_file = get_state_file_path(path);
    if state_file.exists() {
        let content = fs::read_to_string(&state_file)?;
        if let Ok(mut state) = serde_json::from_str::<crate::watcher::DaemonState>(&content) {
            state.status = new_status.to_string();
            fs::write(&state_file, serde_json::to_string_pretty(&state).unwrap())?;
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Implement stats storage and TUI BarChart viewer**

Write the chart view and historical logging in `src/stats.rs`:
```rust
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Default)]
pub struct StatsRecord {
    pub timestamp: u64,
    pub openai: usize,
    pub claude: usize,
    pub gemini: usize,
}

pub fn log_historical_stats(path: &Path, record: &StatsRecord) -> Result<(), std::io::Error> {
    let history_file = crate::daemon::get_state_file_path(path)
        .with_file_name(format!("{}-history.json", crate::config::get_path_hash(path)));

    let mut records: Vec<StatsRecord> = if history_file.exists() {
        let content = fs::read_to_string(&history_file)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    };

    records.push(StatsRecord {
        timestamp: record.timestamp,
        openai: record.openai,
        claude: record.claude,
        gemini: record.gemini,
    });

    fs::write(&history_file, serde_json::to_string_pretty(&records)?)
}

pub fn view_stats_chart(path: &Path) -> Result<(), std::io::Error> {
    // Phase 1 static console print/TUI drawing for stats chart
    let history_file = crate::daemon::get_state_file_path(path)
        .with_file_name(format!("{}-history.json", crate::config::get_path_hash(path)));

    if !history_file.exists() {
        println!("No historical stats found for this project. Start monitoring first.");
        return Ok(());
    }

    let content = fs::read_to_string(&history_file)?;
    let records: Vec<StatsRecord> = serde_json::from_str(&content).unwrap_or_default();

    if records.is_empty() {
        println!("History is empty.");
        return Ok(());
    }

    let latest = &records[records.len() - 1];
    println!("=== Token Usage Stats (Latest Distribution) ===");
    println!("OpenAI GPT-4o:     {}", latest.openai);
    println!("Claude 3.5 Sonnet: {}", latest.claude);
    println!("Gemini 1.5/2.0:    {}", latest.gemini);
    
    // Simple terminal bar graph representation
    let max = latest.openai.max(latest.claude.max(latest.gemini)) as f64;
    let render_bar = |val: usize| -> String {
        if max == 0.0 { return String::new(); }
        let width = ((val as f64 / max) * 40.0) as usize;
        "█".repeat(width)
    };

    println!("\nOpenAI:  [{:<40}]", render_bar(latest.openai));
    println!("Claude:  [{:<40}]", render_bar(latest.claude));
    println!("Gemini:  [{:<40}]", render_bar(latest.gemini));

    Ok(())
}
```

- [ ] **Step 3: Commit**

```bash
git add src/stats.rs src/daemon.rs
git commit -m "feat: implement CLI control modifications and stats logger/grapher"
```

---

### Task 5: TUI Dashboard Integration and Final Wiring

**Files:**
- Modify: `src/main.rs`, `src/ui.rs`

- [ ] **Step 1: Refactor UI view layout to consume daemon state**

Update `src/ui.rs` to render the running elapsed timer, status, active model, and confirmation modals:
```rust
use crate::watcher::DaemonState;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    widgets::{Block, Borders, Cell, LineGauge, Paragraph, Row, Table, Clear},
    Frame,
};

pub const GPT4O_MAX: usize = 128_000;
pub const CLAUDE_MAX: usize = 200_000;
pub const GEMINI_MAX: usize = 1_000_000;

fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, secs)
}

pub fn draw(f: &mut Frame, state: &DaemonState, current_dir: &str, show_pause_modal: bool, show_stop_modal: bool) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(7), // Table Matrix
            Constraint::Min(8),    // Gauges
            Constraint::Length(1), // Footer Menu
        ])
        .split(f.area());

    // 1. Header
    let timer_str = format_duration(state.elapsed_seconds);
    let header_text = vec![
        ratatui::text::Line::from(vec![
            ratatui::text::Span::styled(" ntkn ", Style::default().fg(Color::Cyan).bold()),
            ratatui::text::Span::styled("● ", Style::default().fg(if state.status == "Running" { Color::Green } else { Color::Yellow })),
            ratatui::text::Span::raw(format!("Dir: {} | Model: {} | Time: ", current_dir, state.active_model)),
            ratatui::text::Span::styled(timer_str, Style::default().fg(Color::Green).bold()),
        ])
    ];
    let header_paragraph = Paragraph::new(header_text)
        .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(Color::DarkGray)));
    f.render_widget(header_paragraph, chunks[0]);

    // 2. Table Matrix
    let gpt4o_pct = (state.openai_gpt4o as f64 / GPT4O_MAX as f64) * 100.0;
    let claude_pct = (state.anthropic_claude as f64 / CLAUDE_MAX as f64) * 100.0;
    let gemini_pct = (state.google_gemini as f64 / GEMINI_MAX as f64) * 100.0;

    let active_model_lower = state.active_model.to_lowercase();
    let is_gpt_active = active_model_lower.contains("gpt") || active_model_lower.contains("openai");
    let is_claude_active = active_model_lower.contains("claude") || active_model_lower.contains("anthropic");
    let is_gemini_active = active_model_lower.contains("gemini") || active_model_lower.contains("google");

    let rows = vec![
        Row::new(vec![
            Cell::from(if is_gpt_active { "* OpenAI" } else { "  OpenAI" }).fg(Color::Green),
            Cell::from("GPT-4o"),
            Cell::from(state.openai_gpt4o.to_string()),
            Cell::from(GPT4O_MAX.to_string()),
            Cell::from(format!("{:.2}%", gpt4o_pct)),
        ]).height(1),
        Row::new(vec![
            Cell::from(if is_claude_active { "* Anthropic" } else { "  Anthropic" }).fg(Color::Magenta),
            Cell::from("Claude 3.5 Sonnet"),
            Cell::from(state.anthropic_claude.to_string()),
            Cell::from(CLAUDE_MAX.to_string()),
            Cell::from(format!("{:.2}%", claude_pct)),
        ]).height(1),
        Row::new(vec![
            Cell::from(if is_gemini_active { "* Google" } else { "  Google" }).fg(Color::Yellow),
            Cell::from("Gemini 1.5/2.0"),
            Cell::from(state.google_gemini.to_string()),
            Cell::from(GEMINI_MAX.to_string()),
            Cell::from(format!("{:.2}%", gemini_pct)),
        ]).height(1),
    ];

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(20),
            Constraint::Percentage(30),
            Constraint::Percentage(20),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
        ],
    )
    .header(Row::new(vec!["Provider", "Model Name", "Tokens", "Max Context", "Occupancy"]))
    .block(Block::default().borders(Borders::ALL).title(" Multi-Model Token Matrix "));
    f.render_widget(table, chunks[1]);

    // 3. Gauges
    let gpt_ratio = (state.openai_gpt4o as f64 / GPT4O_MAX as f64).min(1.0).max(0.0);
    let claude_ratio = (state.anthropic_claude as f64 / CLAUDE_MAX as f64).min(1.0).max(0.0);
    let gemini_ratio = (state.google_gemini as f64 / GEMINI_MAX as f64).min(1.0).max(0.0);

    let gpt_gauge = LineGauge::default()
        .block(Block::default().title("GPT-4o (128k)"))
        .filled_style(Style::default().fg(Color::Cyan))
        .ratio(gpt_ratio);
    let claude_gauge = LineGauge::default()
        .block(Block::default().title("Claude 3.5 Sonnet (200k)"))
        .filled_style(Style::default().fg(Color::Magenta))
        .ratio(claude_ratio);
    let gemini_gauge = LineGauge::default()
        .block(Block::default().title("Gemini 1.5 (1M)"))
        .filled_style(Style::default().fg(Color::Yellow))
        .ratio(gemini_ratio);

    let gauge_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(2), Constraint::Length(2)])
        .split(chunks[2]);
    f.render_widget(gpt_gauge, gauge_chunks[0]);
    f.render_widget(claude_gauge, gauge_chunks[1]);
    f.render_widget(gemini_gauge, gauge_chunks[2]);

    // 4. Footer Menu
    let footer_text = Paragraph::new("[p] Pause | [s] Stop | [q] Exit (keeps counting)");
    f.render_widget(footer_text, chunks[3]);

    // Modal Confirmation Dialogs
    if show_pause_modal || show_stop_modal {
        let area = centered_rect(60, 20, f.area());
        f.render_widget(Clear, area); // Clear background under the modal

        let title = if show_pause_modal { " Pause Counting " } else { " Stop Monitoring " };
        let msg = if show_pause_modal {
            "Are you sure you want to pause counting? (y/n)"
        } else {
            "Are you sure you want to stop monitoring? (y/n)"
        };

        let modal_block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(Color::Red));
        
        let p = Paragraph::new(msg)
            .block(modal_block)
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(p, area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
```

- [ ] **Step 2: Update Main router and TUI controller**

Connect all parts together inside `src/main.rs`:
```rust
use std::{io, path::PathBuf, time::Duration};
use crossterm::{
    execute,
    terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    event::{self, Event, KeyCode, KeyModifiers, KeyEventKind},
    cursor,
};
use ratatui::{backend::CrosstermBackend, Terminal};

mod config;
mod scanner;
mod counter;
mod ui;
mod daemon;
mod watcher;
mod stats;

fn main() -> Result<(), io::Error> {
    let args: Vec<String> = std::env::args().collect();
    
    // Command line sub-routing
    if args.len() > 1 {
        match args[1].as_str() {
            "daemon" => {
                if args.len() > 3 && args[2] == "--watch" {
                    let path = PathBuf::from(&args[3]);
                    return daemon::run_daemon(path);
                }
            }
            "pause" => {
                let current_dir = std::env::current_dir()?;
                return daemon::modify_daemon_status(&current_dir, "Paused");
            }
            "resume" => {
                let current_dir = std::env::current_dir()?;
                return daemon::modify_daemon_status(&current_dir, "Running");
            }
            "stop" => {
                let current_dir = std::env::current_dir()?;
                return daemon::modify_daemon_status(&current_dir, "Stopped");
            }
            "stats" | "usage" => {
                let current_dir = std::env::current_dir()?;
                return stats::view_stats_chart(&current_dir);
            }
            _ => {}
        }
    }

    // Default Interactive Startup
    let current_dir = std::env::current_dir()?;
    if !config::verify_trust_interactive(&current_dir)? {
        println!("Exiting.");
        return Ok(());
    }

    // Spawn daemon if not running
    daemon::spawn_daemon(&current_dir)?;

    // Start TUI Monitor
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let current_dir_str = current_dir.to_string_lossy().to_string();
    let state_file = daemon::get_state_file_path(&current_dir);

    let mut show_pause_modal = false;
    let mut show_stop_modal = false;

    loop {
        // Read daemon state from local state JSON
        let state = if state_file.exists() {
            let content = std::fs::read_to_string(&state_file)?;
            serde_json::from_str::<watcher::DaemonState>(&content).unwrap_or_else(|_| create_fallback_state())
        } else {
            create_fallback_state()
        };

        terminal.draw(|f| {
            ui::draw(f, &state, &current_dir_str, show_pause_modal, show_stop_modal);
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if show_pause_modal {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                let _ = daemon::modify_daemon_status(&current_dir, "Paused");
                                show_pause_modal = false;
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                show_pause_modal = false;
                            }
                            _ => {}
                        }
                    } else if show_stop_modal {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                let _ = daemon::modify_daemon_status(&current_dir, "Stopped");
                                show_stop_modal = false;
                                break;
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                show_stop_modal = false;
                            }
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Char('q') => {
                                break;
                            }
                            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                break;
                            }
                            KeyCode::Char('p') => {
                                show_pause_modal = true;
                            }
                            KeyCode::Char('s') => {
                                show_stop_modal = true;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, cursor::Show)?;
    
    println!("ntkn is still counting in the background.");
    println!("To pause counting: run 'ntkn pause'");
    println!("To stop monitoring: run 'ntkn stop'");
    Ok(())
}

fn create_fallback_state() -> watcher::DaemonState {
    watcher::DaemonState {
        pid: 0,
        status: "Running".to_string(),
        start_time: 0,
        elapsed_seconds: 0,
        last_updated: 0,
        active_model: "Loading...".to_string(),
        model_detected: false,
        openai_gpt4o: 0,
        anthropic_claude: 0,
        google_gemini: 0,
    }
}
```

- [ ] **Step 3: Verify execution and clean compilation**

Run: `cargo check`
Expected: Compile successful with zero errors.

- [ ] **Step 4: Commit**

```bash
git add src/ui.rs src/main.rs
git commit -m "feat: complete TUI dashboard implementation with modals and status actions"
```
