use std::{collections::HashMap, path::PathBuf};

const DEFAULT_MODELS_TOML: &str = r#"
[[models]]
label = "Claude Haiku 4.5"
id = "claude-haiku-4-5-20251001"
provider = "anthropic"
context_window = 200000
cost_per_mtok_input = 1.00
cost_per_mtok_output = 5.00

[[models]]
label = "Claude Sonnet 4.6"
id = "claude-sonnet-4-6"
provider = "anthropic"
context_window = 1000000
cost_per_mtok_input = 3.00
cost_per_mtok_output = 15.00

[[models]]
label = "Claude Opus 4.7"
id = "claude-opus-4-7"
provider = "anthropic"
context_window = 1000000
cost_per_mtok_input = 5.00
cost_per_mtok_output = 25.00

[[models]]
label = "Gemini 2.5 Flash"
id = "gemini-2.5-flash"
provider = "openai-compat"
base_url = "https://generativelanguage.googleapis.com/v1beta/openai"
env_key = "GEMINI_API_KEY"
context_window = 1000000
cost_per_mtok_input = 0.30
cost_per_mtok_output = 2.50

[[models]]
label = "Gemini 2.5 Pro"
id = "gemini-2.5-pro"
provider = "openai-compat"
base_url = "https://generativelanguage.googleapis.com/v1beta/openai"
env_key = "GEMINI_API_KEY"
context_window = 1000000
cost_per_mtok_input = 1.25
cost_per_mtok_output = 10.00

[[models]]
label = "Groq Llama 3.1 8B"
id = "llama-3.1-8b-instant"
provider = "openai-compat"
base_url = "https://api.groq.com/openai/v1"
env_key = "GROQ_API_KEY"
context_window = 128000
cost_per_mtok_input = 0.05
cost_per_mtok_output = 0.08

[[models]]
label = "Groq Llama 3.3 70B"
id = "llama-3.3-70b-versatile"
provider = "openai-compat"
base_url = "https://api.groq.com/openai/v1"
env_key = "GROQ_API_KEY"
context_window = 128000
cost_per_mtok_input = 0.59
cost_per_mtok_output = 0.79

[[models]]
label = "OR Auto"
id = "openrouter/auto"
provider = "openai-compat"
base_url = "https://openrouter.ai/api/v1"
env_key = "OPENROUTER_API_KEY"
"#;

#[derive(serde::Deserialize)]
pub struct ModelDef {
    pub label: String,
    pub id: String,
    pub provider: String,
    pub base_url: Option<String>,
    pub env_key: Option<String>,
    pub context_window: Option<u64>,
    pub cost_per_mtok_input: Option<f64>,
    pub cost_per_mtok_output: Option<f64>,
}

#[derive(serde::Deserialize)]
struct ModelsFile {
    models: Vec<ModelDef>,
}

pub fn load_model_defs() -> Vec<ModelDef> {
    let path = pantheon_dir().join("models.toml");

    let contents = if path.exists() {
        std::fs::read_to_string(&path).unwrap_or_default()
    } else {
        // Write default file on first run so users can discover and edit it
        let _ = std::fs::create_dir_all(pantheon_dir());
        let _ = std::fs::write(&path, DEFAULT_MODELS_TOML.trim_start());
        DEFAULT_MODELS_TOML.to_string()
    };

    match toml::from_str::<ModelsFile>(&contents) {
        Ok(f) if !f.models.is_empty() => f.models,
        _ => {
            toml::from_str::<ModelsFile>(DEFAULT_MODELS_TOML)
                .expect("default models TOML is valid")
                .models
        }
    }
}

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
    // Ensure credentials file is only readable by the owner
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if path.exists() {
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
    }
    let contents = std::fs::read_to_string(path).ok()?;
    let map: HashMap<String, String> = serde_json::from_str(&contents).ok()?;
    map.get(env_key).cloned()
}
