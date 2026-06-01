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
