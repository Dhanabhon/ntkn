use notify::{RecursiveMode, Result as NotifyResult, Watcher};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DaemonState {
    pub pid: u32,
    pub status: String,
    pub start_time: u64,
    pub elapsed_seconds: u64,
    pub last_updated: u64,
    pub active_model: String,
    pub model_detected: bool,
    pub openai_gpt4o: usize,
    pub anthropic_claude: usize,
    pub google_gemini: usize,
    pub show_openai: bool,
    pub show_anthropic: bool,
    pub show_gemini: bool,

    #[serde(default)]
    pub openai_model_name: Option<String>,
    #[serde(default)]
    pub openai_limit: Option<usize>,
    #[serde(default)]
    pub anthropic_model_name: Option<String>,
    #[serde(default)]
    pub anthropic_limit: Option<usize>,
    #[serde(default)]
    pub gemini_model_name: Option<String>,
    #[serde(default)]
    pub gemini_limit: Option<usize>,
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
        .map_err(|e| std::io::Error::other(e.to_string()))?
        .as_secs();

    let (has_openai, has_anthropic, has_gemini) = check_env_keys(&watch_path);
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
        show_openai: has_openai,
        show_anthropic: has_anthropic,
        show_gemini: has_gemini,
        openai_model_name: None,
        openai_limit: None,
        anthropic_model_name: None,
        anthropic_limit: None,
        gemini_model_name: None,
        gemini_limit: None,
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
    })
    .map_err(|e| std::io::Error::other(e.to_string()))?;

    watcher
        .watch(&watch_path, RecursiveMode::Recursive)
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    let mut last_tick = std::time::Instant::now();
    let mut prev_status = state.status.clone();

    loop {
        // Wait 250ms or respond to filesystem changes
        let file_changed = rx.recv_timeout(Duration::from_millis(250)).is_ok();

        // Debounce: if file changed, wait a bit and drain mpsc channel to avoid redundant scans
        if file_changed {
            std::thread::sleep(Duration::from_millis(100));
            while rx.try_recv().is_ok() {}
        }

        let mut status_changed = false;
        // Read command modifications made by CLI
        if let Ok(content) = fs::read_to_string(&state_file) {
            if let Ok(updated_state) = serde_json::from_str::<DaemonState>(&content) {
                if updated_state.status == "Stopped" {
                    break;
                }
                if updated_state.status != state.status {
                    state.status = updated_state.status.clone();
                    status_changed = true;
                }
            }
        }

        let mut force_recalculate = false;
        if status_changed && state.status == "Running" && prev_status == "Paused" {
            force_recalculate = true;
        }
        prev_status = state.status.clone();

        if state.status == "Running" {
            let mut state_changed = false;

            let elapsed = last_tick.elapsed();
            if elapsed >= Duration::from_secs(1) {
                let secs = elapsed.as_secs();
                state.elapsed_seconds += secs;
                last_tick += Duration::from_secs(secs);
                state_changed = true;
            }

            if file_changed || force_recalculate {
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
                state_changed = true;
            }

            if state_changed || status_changed {
                state.last_updated = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| std::io::Error::other(e.to_string()))?
                    .as_secs();
                write_state(&state_file, &state)?;
            }
        } else {
            if status_changed {
                state.last_updated = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| std::io::Error::other(e.to_string()))?
                    .as_secs();
                write_state(&state_file, &state)?;
            }
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

fn detect_aider_conf_model(path: &Path) -> Option<String> {
    let conf_file = path.join(".aider.conf.yml");
    if conf_file.exists() {
        if let Ok(content) = std::fs::read_to_string(conf_file) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("model:") {
                    let model = trimmed["model:".len()..].trim();
                    let clean = model.trim_matches(|c| c == '\'' || c == '"');
                    if !clean.is_empty() {
                        return Some(clean.to_string());
                    }
                }
            }
        }
    }
    None
}

fn detect_aider_model(path: &Path) -> Option<String> {
    let history_file = path.join(".aider.chat.history.md");
    if history_file.exists() {
        if let Ok(content) = std::fs::read_to_string(history_file) {
            for line in content.lines().rev() {
                let trimmed = line.trim();
                if trimmed.starts_with("# Model:") {
                    let model = trimmed["# Model:".len()..].trim();
                    if !model.is_empty() {
                        return Some(model.to_string());
                    }
                } else if trimmed.contains("Model:") {
                    if let Some(idx) = trimmed.find("Model:") {
                        let model = trimmed[idx + "Model:".len()..].trim();
                        if !model.is_empty() {
                            let clean: String = model
                                .chars()
                                .filter(|&c| c != '`' && c != '[' && c != ']')
                                .collect();
                            return Some(clean);
                        }
                    }
                }
            }
        }
    }
    None
}

fn detect_active_model(path: &Path) -> Option<String> {
    let local_config = crate::config::load_local_config(path);
    if let Some(ref model) = local_config.default_model {
        return Some(model.clone());
    }
    if let Ok(val) = std::env::var("AIDER_MODEL") {
        return Some(val);
    }
    if let Some(model) = detect_aider_conf_model(path) {
        return Some(model);
    }
    if let Some(model) = detect_aider_model(path) {
        return Some(model);
    }
    None
}

fn recalculate_state(path: &Path, state: &mut DaemonState) {
    let scanned_text = crate::scanner::ProjectScanner::scan_project(path);
    let counts = crate::counter::TokenCounter::calculate_all(&scanned_text);
    state.openai_gpt4o = counts.openai_gpt4o;
    state.anthropic_claude = counts.anthropic_claude;
    state.google_gemini = counts.google_gemini;

    // Detect model fallback logic
    if let Some(model) = detect_active_model(path) {
        state.active_model = model;
        state.model_detected = true;
    } else {
        state.active_model = "Unrecognized".to_string();
        state.model_detected = false;
    }

    // Resolve model details for each provider
    let local_config = crate::config::load_local_config(path);
    let custom_limits = &local_config.custom_limits;

    let openai_details =
        crate::models::resolve_model_details(&state.active_model, "openai", custom_limits);
    state.openai_model_name = Some(openai_details.display_name);
    state.openai_limit = Some(openai_details.limit);

    let anthropic_details =
        crate::models::resolve_model_details(&state.active_model, "anthropic", custom_limits);
    state.anthropic_model_name = Some(anthropic_details.display_name);
    state.anthropic_limit = Some(anthropic_details.limit);

    let gemini_details =
        crate::models::resolve_model_details(&state.active_model, "google", custom_limits);
    state.gemini_model_name = Some(gemini_details.display_name);
    state.gemini_limit = Some(gemini_details.limit);

    let (has_openai, has_anthropic, has_gemini) = check_env_keys(path);
    state.show_openai = has_openai;
    state.show_anthropic = has_anthropic;
    state.show_gemini = has_gemini;
}

fn check_env_keys(path: &Path) -> (bool, bool, bool) {
    let mut has_openai = std::env::var("OPENAI_API_KEY").is_ok();
    let mut has_anthropic = std::env::var("ANTHROPIC_API_KEY").is_ok();
    let mut has_gemini = std::env::var("GEMINI_API_KEY").is_ok();

    // 1. Check project-local .env file
    let env_file = path.join(".env");
    if env_file.exists() {
        if let Ok(content) = std::fs::read_to_string(env_file) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('#') {
                    continue;
                }
                if let Some(pos) = trimmed.find('=') {
                    let key = trimmed[..pos].trim();
                    let val = trimmed[pos + 1..].trim();
                    if !val.is_empty() && val != "\"\"" && val != "''" {
                        if key == "OPENAI_API_KEY" {
                            has_openai = true;
                        }
                        if key == "ANTHROPIC_API_KEY" {
                            has_anthropic = true;
                        }
                        if key == "GEMINI_API_KEY" {
                            has_gemini = true;
                        }
                    }
                }
            }
        }
    }

    // 2. Check active model from config/env
    if let Some(model) = detect_active_model(path) {
        let model_lower = model.to_lowercase();
        if model_lower.contains("gpt")
            || model_lower.contains("openai")
            || model_lower.contains("o1")
            || model_lower.contains("o3")
        {
            has_openai = true;
        }
        if model_lower.contains("claude") || model_lower.contains("anthropic") {
            has_anthropic = true;
        }
        if model_lower.contains("gemini") || model_lower.contains("google") {
            has_gemini = true;
        }
    }

    // 3. Check for local agent files in this directory
    if path.join(".aider.chat.history.md").exists() || path.join(".aider.conf.yml").exists() {
        has_openai = true;
    }
    if path.join(".cursorrules").exists() {
        has_openai = true;
    }
    if path.join(".clauderc").exists() || path.join(".claude").exists() {
        has_anthropic = true;
    }

    // If none are detected, default to showing all of them
    if !has_openai && !has_anthropic && !has_gemini {
        return (true, true, true);
    }

    (has_openai, has_anthropic, has_gemini)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_state_backward_compatibility() {
        let old_json = r#"{
            "pid": 1234,
            "status": "Running",
            "start_time": 100,
            "elapsed_seconds": 45,
            "last_updated": 200,
            "active_model": "gpt-3.5-turbo",
            "model_detected": true,
            "openai_gpt4o": 1000,
            "anthropic_claude": 2000,
            "google_gemini": 3000,
            "show_openai": true,
            "show_anthropic": true,
            "show_gemini": true
        }"#;

        let state: DaemonState = serde_json::from_str(old_json).unwrap();
        assert_eq!(state.pid, 1234);
        assert_eq!(state.active_model, "gpt-3.5-turbo");
        assert_eq!(state.openai_model_name, None);
        assert_eq!(state.openai_limit, None);
        assert_eq!(state.anthropic_model_name, None);
        assert_eq!(state.anthropic_limit, None);
        assert_eq!(state.gemini_model_name, None);
        assert_eq!(state.gemini_limit, None);
    }

    #[test]
    fn test_notify_events() {
        let temp_dir = std::env::temp_dir().join("test_ntkn_notify");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |res: NotifyResult<notify::Event>| {
            if res.is_ok() {
                let _ = tx.send(());
            }
        })
        .unwrap();

        watcher
            .watch(&temp_dir, notify::RecursiveMode::Recursive)
            .unwrap();

        std::fs::write(temp_dir.join("test.txt"), "hello").unwrap();

        let received = rx.recv_timeout(Duration::from_secs(2)).is_ok();
        assert!(received, "Should receive file change event");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
