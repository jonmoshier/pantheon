import pytest
from unittest.mock import patch
from pantheon.router import classify, pick_model


# --- classify ---

def test_classify_short_casual_is_free():
    assert classify("what is the capital of France?") == "free"

def test_classify_summarize_is_free():
    assert classify("summarize this article for me") == "free"

def test_classify_tldr_is_free():
    assert classify("tldr of this doc") == "free"

def test_classify_long_prompt_is_cheap():
    assert classify("a" * 501) == "cheap"

def test_classify_debug_is_full():
    assert classify("debug this function, it keeps crashing") == "full"

def test_classify_refactor_is_full():
    assert classify("refactor this module to use dependency injection") == "full"

def test_classify_implement_is_full():
    assert classify("implement a binary search tree in Python") == "full"

def test_classify_default_short_unknown_is_free():
    assert classify("hey") == "free"

def test_classify_case_insensitive():
    assert classify("Summarize this for me") == "free"
    assert classify("DEBUG this please") == "full"


# --- pick_model ---

def test_pick_model_returns_provider_and_model_string():
    with patch("pantheon.router.enabled_providers", return_value=["claude-haiku"]):
        provider_id, model = pick_model("what is Python?")
        assert provider_id == "claude-haiku"
        assert "haiku" in model.lower()

def test_pick_model_prefers_free_tier_for_simple_prompt():
    with patch("pantheon.router.enabled_providers", return_value=["gemini-flash", "claude-haiku", "claude-sonnet"]):
        provider_id, _ = pick_model("what is the weather?")
        assert provider_id == "gemini-flash"

def test_pick_model_escalates_to_full_for_complex_prompt():
    with patch("pantheon.router.enabled_providers", return_value=["gemini-flash", "claude-haiku", "claude-sonnet"]):
        provider_id, _ = pick_model("refactor this entire codebase")
        assert provider_id == "claude-sonnet"

def test_pick_model_escalates_when_preferred_tier_unavailable():
    # Only haiku available; complex prompt wants full but escalates to best available
    with patch("pantheon.router.enabled_providers", return_value=["claude-haiku"]):
        provider_id, _ = pick_model("architect a distributed system")
        assert provider_id == "claude-haiku"

def test_pick_model_raises_when_no_providers():
    with patch("pantheon.router.enabled_providers", return_value=[]):
        with pytest.raises(RuntimeError, match="No providers configured"):
            pick_model("hello")

def test_pick_model_uses_cheap_when_free_unavailable():
    with patch("pantheon.router.enabled_providers", return_value=["claude-haiku", "claude-sonnet"]):
        provider_id, _ = pick_model("summarize this")
        assert provider_id == "claude-haiku"
