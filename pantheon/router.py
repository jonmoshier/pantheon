from pantheon.config import PROVIDERS, TIER_ORDER, enabled_providers

# Maps task keywords to a skill
_SKILL_SIGNALS: list[tuple[str, list[str]]] = [
    ("code",            ["debug", "refactor", "implement", "function", "class", "bug", "error", "syntax",
                         "write a", "write me a", "fix this", "review my code"]),
    ("reasoning",       ["architect", "design", "analyze", "compare", "tradeoff", "pros and cons",
                         "nuanced", "complex", "explain why", "should i"]),
    ("summarization",   ["summarize", "summary", "tldr", "rephrase", "reword", "shorten",
                         "explain briefly", "key points"]),
    ("structured_output", ["json", "csv", "table", "extract", "parse", "format as", "list of"]),
    ("creative",        ["write a story", "write a poem", "brainstorm", "generate ideas", "creative"]),
    ("speed",           ["what is", "what are", "define", "who is", "when did", "translate", "quick"]),
]

# Maps skill to the minimum tier where it makes sense to escalate
_SKILL_MIN_TIER: dict[str, str] = {
    "code":             "cheap",
    "reasoning":        "full",
    "summarization":    "free",
    "structured_output": "cheap",
    "creative":         "full",
    "speed":            "free",
    "routing":          "free",
}


def classify(prompt: str) -> tuple[str, str]:
    """Return (tier, skill) for this prompt."""
    lower = prompt.lower()

    scores: dict[str, int] = {}
    for skill, keywords in _SKILL_SIGNALS:
        count = sum(1 for kw in keywords if kw in lower)
        if count:
            scores[skill] = count

    if not scores:
        # Long prompts without clear signals default to reasoning
        skill = "reasoning" if len(prompt) > 500 else "speed"
    else:
        skill = max(scores, key=lambda s: scores[s])

    tier = _SKILL_MIN_TIER.get(skill, "free")
    return tier, skill


def pick_model(prompt: str) -> tuple[str, str]:
    """Return (provider_id, litellm_model_string) for this prompt."""
    tier, skill = classify(prompt)
    available = enabled_providers()

    if not available:
        raise RuntimeError("No providers configured. Run: pan auth add")

    tier_index = TIER_ORDER.index(tier)
    tiers_to_try = TIER_ORDER[tier_index:] + TIER_ORDER[:tier_index]

    # Prefer skill match at lowest viable tier; fall back to any provider
    for t in tiers_to_try:
        for pid in available:
            meta = PROVIDERS[pid]
            if meta["tier"] == t and skill in meta.get("skills", []):
                return pid, meta["model"]

    # No skill match — fall back to tier-only selection
    for t in tiers_to_try:
        for pid in available:
            if PROVIDERS[pid]["tier"] == t:
                return pid, PROVIDERS[pid]["model"]

    raise RuntimeError("No providers configured. Run: pan auth add")
