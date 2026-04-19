# Pantheon

Cost-aware, skill-aware LLM router for the terminal. One interface to many models — each request goes to the right one.

## Vision

Most people pick one LLM and use it for everything. That's expensive for simple tasks and underpowered for hard ones. Pantheon routes each message to the cheapest model that has the right skill for the job.

The long-term goal is a multi-LLM conversation where models delegate to each other — a fast cheap model handles summarization, a reasoning model handles architecture questions, and a router model decides who gets what. Today, routing is static rules. Tomorrow, routing is itself a skill assigned to a provider.

## Core concepts

### Tiers
Cost levels: `free` → `cheap` → `full`. Prefer the lowest tier that satisfies the skill requirement.

### Skills
Each provider has a list of skills it's good at. The classifier detects the task type and maps it to a skill. `pick_model` finds the best provider for that skill at the lowest tier.

| Skill | Description |
|---|---|
| `routing` | Classifying and delegating requests to other models |
| `code` | Writing, debugging, refactoring code |
| `reasoning` | Multi-step logic, analysis, architecture |
| `summarization` | Condensing and rephrasing text |
| `speed` | Fast responses to simple factual questions |
| `structured_output` | JSON extraction, data formatting |
| `creative` | Writing, brainstorming, open-ended generation |

### Routing skill
`routing` is reserved for when we replace static rules with a live LLM classifier. The static classifier fills this role today. When activated, pantheon will pick the provider with the `routing` skill to classify each incoming message before dispatching it.

## Architecture

```
user message
    │
    ▼
classify(prompt) → (tier, skill)       ← static rules today, routing-skill LLM later
    │
    ▼
pick_model(tier, skill) → provider     ← scores by skill match + tier
    │
    ▼
provider_client.complete(model, messages)
```

## Provider philosophy
A provider can have multiple skills. Skill match takes priority; tier is the tiebreaker. A lower-tier provider that matches the skill beats a higher-tier provider that doesn't.

## What this is not
- Not a load balancer (no redundancy/failover goals)
- Not an agent framework (delegation is a future milestone, not the current shape)
- Not a proxy (local CLI tool only)
