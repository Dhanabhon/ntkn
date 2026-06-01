use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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
    use sha2::{Digest, Sha256};
    let absolute = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut hasher = Sha256::new();
    hasher.update(absolute.to_string_lossy().as_bytes());
    format!("{:x}", hasher.finalize())
}

fn get_global_config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("ntkn")
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
    println!(
        "Trusting the directory allows project-local config, hooks, and exec policies to load."
    );
    println!();

    let options = vec!["Yes, continue", "No, quit"];
    let mut selected = 0;

    let print_menu = |selected_idx: usize| {
        print!("\r");
        for (i, opt) in options.iter().enumerate() {
            if i == selected_idx {
                print!("> \x1b[36m\x1b[1m{}\x1b[0m   ", opt);
            } else {
                print!("  {}   ", opt);
            }
        }
        use std::io::Write;
        std::io::stdout().flush().unwrap();
    };

    print_menu(selected);

    crossterm::terminal::enable_raw_mode()?;

    let result = loop {
        if crossterm::event::poll(std::time::Duration::from_millis(100))? {
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                if key.kind == crossterm::event::KeyEventKind::Press {
                    match key.code {
                        crossterm::event::KeyCode::Left | crossterm::event::KeyCode::Up => {
                            if selected > 0 {
                                selected -= 1;
                                print_menu(selected);
                            }
                        }
                        crossterm::event::KeyCode::Right | crossterm::event::KeyCode::Down => {
                            if selected < options.len() - 1 {
                                selected += 1;
                                print_menu(selected);
                            }
                        }
                        crossterm::event::KeyCode::Enter => {
                            break selected == 0;
                        }
                        crossterm::event::KeyCode::Char('c')
                            if key
                                .modifiers
                                .contains(crossterm::event::KeyModifiers::CONTROL) =>
                        {
                            break false;
                        }
                        _ => {}
                    }
                }
            }
        }
    };

    crossterm::terminal::disable_raw_mode()?;
    println!("\n");

    if result {
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
