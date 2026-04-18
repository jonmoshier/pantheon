import pytest
from unittest.mock import patch
from pantheon.router import classify, pick_model


# --- classify ---

def test_classify_short_casual_returns_speed():
    tier, skill = classify("what is the capital of France?")
    assert tier == "free"
    assert skill == "speed"

def test_classify_summarize():
    tier, skill = classify("summarize this article for me")
    assert tier == "free"
    assert skill == "summarization"

def test_classify_tldr():
    tier, skill = classify("tldr of this doc")
    assert tier == "free"
    assert skill == "summarization"

def test_classify_long_prompt_no_signals():
    tier, skill = classify("a" * 501)
    assert tier == "full"
    assert skill == "reasoning"

def test_classify_debug_is_code():
    tier, skill = classify("debug this function, it keeps crashing")
    assert tier == "cheap"
    assert skill == "code"

def test_classify_refactor_is_code():
    tier, skill = classify("refactor this module to use dependency injection")
    assert tier == "cheap"
    assert skill == "code"

def test_classify_implement_is_code():
    tier, skill = classify("implement a binary search tree in Python")
    assert tier == "cheap"
    assert skill == "code"

def test_classify_architect_is_reasoning():
    tier, skill = classify("architect a distributed system for high availability")
    assert tier == "full"
    assert skill == "reasoning"

def test_classify_default_short_unknown_is_speed():
    tier, skill = classify("hey")
    assert tier == "free"
    assert skill == "speed"

def test_classify_case_insensitive():
    _, skill = classify("Summarize this for me")
    assert skill == "summarization"
    _, skill = classify("DEBUG this please")
    assert skill == "code"

def test_classify_returns_tuple():
    result = classify("hello")
    assert isinstance(result, tuple)
    assert len(result) == 2


# --- pick_model ---

def test_pick_model_returns_provider_and_model_string():
    with patch("pantheon.router.enabled_providers", return_value=["claude-haiku"]):
        provider_id, model = pick_model("what is Python?")
        assert provider_id == "claude-haiku"
        assert "haiku" in model.lower()

def test_pick_model_prefers_skill_match_at_free_tier():
    with patch("pantheon.router.enabled_providers", return_value=["gemini-flash", "claude-haiku", "claude-sonnet"]):
        provider_id, _ = pick_model("what is the weather?")
        assert provider_id == "gemini-flash"

def test_pick_model_routes_code_to_haiku_before_sonnet():
    with patch("pantheon.router.enabled_providers", return_value=["gemini-flash", "claude-haiku", "claude-sonnet"]):
        provider_id, _ = pick_model("refactor this entire codebase")
        assert provider_id == "claude-haiku"

def test_pick_model_routes_reasoning_to_sonnet():
    with patch("pantheon.router.enabled_providers", return_value=["gemini-flash", "claude-haiku", "claude-sonnet"]):
        provider_id, _ = pick_model("architect a distributed system")
        assert provider_id == "claude-sonnet"

def test_pick_model_escalates_when_preferred_tier_unavailable():
    with patch("pantheon.router.enabled_providers", return_value=["claude-haiku"]):
        provider_id, _ = pick_model("architect a distributed system")
        assert provider_id == "claude-haiku"

def test_pick_model_raises_when_no_providers():
    with patch("pantheon.router.enabled_providers", return_value=[]):
        with pytest.raises(RuntimeError, match="No providers configured"):
            pick_model("hello")

def test_pick_model_summarization_prefers_gemini_over_haiku():
    with patch("pantheon.router.enabled_providers", return_value=["gemini-flash", "claude-haiku"]):
        provider_id, _ = pick_model("summarize this document")
        assert provider_id == "gemini-flash"
