import json
import pytest
from pathlib import Path
from unittest.mock import patch


@pytest.fixture
def tmp_config(tmp_path, monkeypatch):
    """Point all config paths at a temp directory."""
    monkeypatch.setattr("pantheon.config.CONFIG_DIR", tmp_path)
    monkeypatch.setattr("pantheon.config.CREDENTIALS_FILE", tmp_path / "credentials.json")
    monkeypatch.setattr("pantheon.config.CONFIG_FILE", tmp_path / "config.json")
    return tmp_path


# --- load_credentials ---

def test_load_credentials_empty_when_nothing_configured(tmp_config, monkeypatch):
    monkeypatch.delenv("GEMINI_API_KEY", raising=False)
    monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)
    from pantheon.config import load_credentials
    assert load_credentials() == {}

def test_load_credentials_reads_from_file(tmp_config):
    (tmp_config / "credentials.json").write_text(json.dumps({"ANTHROPIC_API_KEY": "sk-test"}))
    from pantheon.config import load_credentials
    creds = load_credentials()
    assert creds["ANTHROPIC_API_KEY"] == "sk-test"

def test_env_var_overrides_file(tmp_config, monkeypatch):
    (tmp_config / "credentials.json").write_text(json.dumps({"ANTHROPIC_API_KEY": "from-file"}))
    monkeypatch.setenv("ANTHROPIC_API_KEY", "from-env")
    from pantheon.config import load_credentials
    creds = load_credentials()
    assert creds["ANTHROPIC_API_KEY"] == "from-env"


# --- save_credential ---

def test_save_credential_creates_file(tmp_config):
    from pantheon.config import save_credential
    save_credential("ANTHROPIC_API_KEY", "sk-abc")
    data = json.loads((tmp_config / "credentials.json").read_text())
    assert data["ANTHROPIC_API_KEY"] == "sk-abc"

def test_save_credential_merges_existing(tmp_config):
    (tmp_config / "credentials.json").write_text(json.dumps({"GEMINI_API_KEY": "g-key"}))
    from pantheon.config import save_credential
    save_credential("ANTHROPIC_API_KEY", "sk-abc")
    data = json.loads((tmp_config / "credentials.json").read_text())
    assert data["GEMINI_API_KEY"] == "g-key"
    assert data["ANTHROPIC_API_KEY"] == "sk-abc"

def test_save_credential_sets_restrictive_permissions(tmp_config):
    from pantheon.config import save_credential
    save_credential("ANTHROPIC_API_KEY", "sk-abc")
    mode = (tmp_config / "credentials.json").stat().st_mode & 0o777
    assert mode == 0o600


# --- enabled_providers / is_configured ---

def test_is_configured_false_with_no_config(tmp_config, monkeypatch):
    monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)
    from pantheon.config import is_configured
    assert not is_configured()

def test_is_configured_true_when_provider_enabled_with_key(tmp_config):
    (tmp_config / "config.json").write_text(json.dumps({"enabled_providers": ["claude-haiku"]}))
    (tmp_config / "credentials.json").write_text(json.dumps({"ANTHROPIC_API_KEY": "sk-test"}))
    from pantheon.config import is_configured
    assert is_configured()

def test_is_configured_false_when_enabled_but_no_key(tmp_config, monkeypatch):
    monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)
    (tmp_config / "config.json").write_text(json.dumps({"enabled_providers": ["claude-haiku"]}))
    from pantheon.config import is_configured
    assert not is_configured()

def test_enabled_providers_excludes_missing_keys(tmp_config, monkeypatch):
    monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)
    (tmp_config / "config.json").write_text(json.dumps({
        "enabled_providers": ["claude-haiku", "gemini-flash"]
    }))
    (tmp_config / "credentials.json").write_text(json.dumps({"GEMINI_API_KEY": "g-key"}))
    from pantheon.config import enabled_providers
    assert enabled_providers() == ["gemini-flash"]
