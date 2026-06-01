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

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?
        .as_secs();

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
    let record = crate::stats::StatsRecord {
        timestamp: now_secs,
        openai: state.openai_gpt4o,
        claude: state.anthropic_claude,
        gemini: state.google_gemini,
    };
    let _ = crate::stats::log_historical_stats(&watch_path, &record);
    write_state(&state_file, &state)?;

    // Setup notify directory watcher
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res: NotifyResult<notify::Event>| {
        if res.is_ok() {
            let _ = tx.send(());
        }
    }).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    watcher.watch(&watch_path, RecursiveMode::Recursive)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    let mut last_tick = std::time::Instant::now();
    loop {
        // Wait 250ms or respond to filesystem changes
        let file_changed = rx.recv_timeout(Duration::from_millis(250)).is_ok();

        // Debounce: if file changed, wait a bit and drain mpsc channel to avoid redundant scans
        if file_changed {
            std::thread::sleep(Duration::from_millis(100));
            while rx.try_recv().is_ok() {}
        }

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
                let secs = elapsed.as_secs();
                state.elapsed_seconds += secs;
                last_tick += Duration::from_secs(secs);
            }

            if file_changed {
                recalculate_state(&watch_path, &mut state);
                let record = crate::stats::StatsRecord {
                    timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                    openai: state.openai_gpt4o,
                    claude: state.anthropic_claude,
                    gemini: state.google_gemini,
                };
                let _ = crate::stats::log_historical_stats(&watch_path, &record);
            }
            
            state.last_updated = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?
                .as_secs();
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
    let temp_path = file_path.with_extension("tmp");
    fs::write(&temp_path, content)?;
    fs::rename(temp_path, file_path)?;
    Ok(())
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
