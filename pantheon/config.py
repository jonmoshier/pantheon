import json
import os
from pathlib import Path
from dataclasses import dataclass, field

CONFIG_DIR = Path.home() / ".pantheon"
CREDENTIALS_FILE = CONFIG_DIR / "credentials.json"
CONFIG_FILE = CONFIG_DIR / "config.json"

PROVIDERS = {
    "gemini-flash": {
        "label": "Gemini 2.0 Flash",
        "model": "gemini/gemini-flash-latest",
        "tier": "free",
        "env_key": "GEMINI_API_KEY",
        "key_url": "https://aistudio.google.com/app/apikey",
        "skills": ["speed", "summarization", "routing"],
    },
    "groq-llama": {
        "label": "Groq / Llama 3",
        "model": "groq/llama3-70b-8192",
        "tier": "free",
        "env_key": "GROQ_API_KEY",
        "key_url": "https://console.groq.com/keys",
        "skills": ["speed", "routing"],
    },
    "claude-haiku": {
        "label": "Claude Haiku",
        "model": "claude-haiku-4-5-20251001",
        "tier": "cheap",
        "env_key": "ANTHROPIC_API_KEY",
        "key_url": "https://console.anthropic.com/settings/keys",
        "skills": ["code", "summarization", "routing"],
    },
    "claude-sonnet": {
        "label": "Claude Sonnet",
        "model": "claude-sonnet-4-6",
        "tier": "full",
        "env_key": "ANTHROPIC_API_KEY",
        "key_url": "https://console.anthropic.com/settings/keys",
        "skills": ["code", "reasoning", "creative"],
    },
    "gpt-4o-mini": {
        "label": "GPT-4o mini",
        "model": "gpt-4o-mini",
        "tier": "cheap",
        "env_key": "OPENAI_API_KEY",
        "key_url": "https://platform.openai.com/api-keys",
        "skills": ["structured_output", "speed"],
    },
}

TIER_ORDER = ["free", "cheap", "full"]


def load_credentials() -> dict:
    creds = {}
    # File-based credentials
    if CREDENTIALS_FILE.exists():
        creds.update(json.loads(CREDENTIALS_FILE.read_text()))
    # Env vars override
    for provider, meta in PROVIDERS.items():
        val = os.environ.get(meta["env_key"])
        if val:
            creds[meta["env_key"]] = val
    return creds


def save_credential(env_key: str, value: str):
    CONFIG_DIR.mkdir(mode=0o700, exist_ok=True)
    creds = {}
    if CREDENTIALS_FILE.exists():
        creds = json.loads(CREDENTIALS_FILE.read_text())
    creds[env_key] = value
    CREDENTIALS_FILE.write_text(json.dumps(creds, indent=2))
    CREDENTIALS_FILE.chmod(0o600)


def load_config() -> dict:
    if CONFIG_FILE.exists():
        return json.loads(CONFIG_FILE.read_text())
    return {"enabled_providers": [], "routing": "auto"}


def save_config(config: dict):
    CONFIG_DIR.mkdir(mode=0o700, exist_ok=True)
    CONFIG_FILE.write_text(json.dumps(config, indent=2))


def enabled_providers() -> list[str]:
    creds = load_credentials()
    config = load_config()
    enabled = []
    for provider_id in config.get("enabled_providers", []):
        meta = PROVIDERS.get(provider_id)
        if meta and creds.get(meta["env_key"]):
            enabled.append(provider_id)
    return enabled


def is_configured() -> bool:
    return len(enabled_providers()) > 0
