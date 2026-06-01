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
