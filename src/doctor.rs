use crate::config;
use crate::daemon;
use std::fs;
use std::path::Path;

pub fn run_diagnostics(path: &Path) -> Vec<String> {
    let mut results = Vec::new();

    results.push("--- Global Configuration ---".to_string());
    let global_dir = config::get_global_config_dir();
    if global_dir.exists() {
        results.push(format!(
            "  [OK] Global config directory exists: {}",
            global_dir.display()
        ));
    } else {
        results.push(format!(
            "  [WARN] Global config directory does not exist at: {}",
            global_dir.display()
        ));
    }

    let registry_file = global_dir.join("trusted_paths.toml");
    if config::is_path_trusted(path, &registry_file) {
        results.push("  [OK] Project directory is trusted.".to_string());
    } else {
        results.push("  [WARN] Project directory is untrusted.".to_string());
    }

    results.push("".to_string());
    results.push("--- Daemon Status ---".to_string());
    let is_running = daemon::is_daemon_running(path);
    let pid_file = daemon::get_pid_file_path(path);
    if is_running {
        let pid_str = fs::read_to_string(&pid_file)
            .unwrap_or_default()
            .trim()
            .to_string();
        results.push(format!("  [OK] Daemon is running. (PID: {})", pid_str));

        let state_file = daemon::get_state_file_path(path);
        if state_file.exists() {
            if let Ok(content) = fs::read_to_string(&state_file) {
                if let Ok(state) = serde_json::from_str::<crate::watcher::DaemonState>(&content) {
                    results.push(format!("  [OK] Daemon status: {}", state.status));
                    results.push(format!(
                        "  [OK] Daemon active model: {}",
                        state.active_model
                    ));
                }
            }
        }
    } else {
        results.push("  [WARN] Daemon is NOT running.".to_string());
    }

    results.push("".to_string());
    results.push("--- Local Configuration (.ntkn.toml) ---".to_string());
    let config_file = path.join(".ntkn.toml");
    if config_file.exists() {
        match fs::read_to_string(&config_file) {
            Ok(content) => match toml::from_str::<config::LocalConfig>(&content) {
                Ok(cfg) => {
                    results
                        .push("  [OK] Local config `.ntkn.toml` parsed successfully.".to_string());
                    if let Some(ref model) = cfg.default_model {
                        results.push(format!("    - Default Model: {}", model));
                    } else {
                        results.push("    - Default Model: Not specified".to_string());
                    }
                    if let Some(ref ignored) = cfg.ignored_dirs {
                        results.push(format!("    - Ignored directories: {:?}", ignored));
                    } else {
                        results.push("    - Ignored directories: None specified".to_string());
                    }
                }
                Err(e) => {
                    results.push(format!("  [ERROR] Failed to parse `.ntkn.toml`: {}", e));
                }
            },
            Err(e) => {
                results.push(format!("  [ERROR] Failed to read `.ntkn.toml`: {}", e));
            }
        }
    } else {
        results
            .push("  [OK] No local `.ntkn.toml` configuration file found (optional).".to_string());
    }

    results.push("".to_string());
    results.push("--- API Keys Configuration ---".to_string());

    let mut has_openai = std::env::var("OPENAI_API_KEY").is_ok();
    let mut has_anthropic = std::env::var("ANTHROPIC_API_KEY").is_ok();
    let mut has_gemini = std::env::var("GEMINI_API_KEY").is_ok();

    let env_file = path.join(".env");
    let mut env_found = false;
    if env_file.exists() {
        env_found = true;
        if let Ok(content) = fs::read_to_string(&env_file) {
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

    if env_found {
        results.push("  [OK] Found project-local `.env` file.".to_string());
    } else {
        results.push("  [INFO] No project-local `.env` file.".to_string());
    }

    let show_key_status = |name: &str, configured: bool| -> String {
        if configured {
            format!("  [OK] {} API Key: Configured", name)
        } else {
            format!("  [WARN] {} API Key: NOT Configured", name)
        }
    };

    results.push(show_key_status("OpenAI", has_openai));
    results.push(show_key_status("Anthropic", has_anthropic));
    results.push(show_key_status("Gemini", has_gemini));

    results.push("".to_string());
    results.push("Press any key or ESC to close diagnostics...".to_string());

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_diagnostics_does_not_panic() {
        let temp_dir = std::env::temp_dir().join("ntkn_test_doctor");
        fs::create_dir_all(&temp_dir).unwrap();

        let results = run_diagnostics(&temp_dir);
        assert!(!results.is_empty());
        assert!(
            results
                .iter()
                .any(|line| line.contains("Global Configuration"))
        );

        fs::remove_dir_all(&temp_dir).unwrap();
    }
}
