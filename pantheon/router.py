from pantheon.config import PROVIDERS, TIER_ORDER, enabled_providers

CHEAP_KEYWORDS = [
    "summarize", "summary", "rephrase", "reword", "translate",
    "what is", "what are", "define", "list", "simple", "quick",
    "tldr", "explain briefly",
]

FULL_KEYWORDS = [
    "debug", "refactor", "architect", "design", "analyze", "compare",
    "write a", "implement", "complex", "nuanced", "review my",
]


def classify(prompt: str) -> str:
    """Return a tier: 'free', 'cheap', or 'full'."""
    lower = prompt.lower()

    if any(kw in lower for kw in FULL_KEYWORDS):
        return "full"
    if len(prompt) > 500:
        return "cheap"
    if any(kw in lower for kw in CHEAP_KEYWORDS):
        return "free"

    return "free"  # default to cheapest


def pick_model(prompt: str) -> tuple[str, str]:
    """Return (provider_id, litellm_model_string) for this prompt."""
    tier = classify(prompt)
    available = enabled_providers()

    tier_index = TIER_ORDER.index(tier)

    # Try requested tier first, then escalate if unavailable
    for t in TIER_ORDER[tier_index:] + TIER_ORDER[:tier_index]:
        for pid in available:
            if PROVIDERS[pid]["tier"] == t:
                return pid, PROVIDERS[pid]["model"]

    raise RuntimeError("No providers configured. Run: pan auth add")
