use std::{collections::HashMap, path::PathBuf};

pub fn pantheon_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".pantheon")
}

pub fn load_api_key(env_key: &str) -> Option<String> {
    if let Ok(val) = std::env::var(env_key) {
        if !val.is_empty() {
            return Some(val);
        }
    }
    let path = pantheon_dir().join("credentials.json");
    let contents = std::fs::read_to_string(path).ok()?;
    let map: HashMap<String, String> = serde_json::from_str(&contents).ok()?;
    map.get(env_key).cloned()
}
