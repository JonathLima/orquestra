---
name: orquestra-planner
description: Use when turning requirements into an Orquestra canonical DAG plan with executable tickets, dependencies, assigned real skills, waves, and verification policy.
version: 0.4.0
---

# Orquestra Planner

## Overview

Create plans that the Rust runtime can validate and execute. The planner proposes the DAG; `orquestra plan validate` and `orquestra plan waves` are the source of truth for correctness.

## Inputs

- User requirements and acceptance criteria.
- `.orquestra/skills_inventory.json` or `.orquestra/skills_inventory.md`.
- Existing project constraints and test commands.
- User model preferences, host CLI, cost sensitivity, and whether current-date web research is allowed.
- Existing `.orquestra/memory/` facts when present.

## Ticket Rules

Each ticket must:

- Have a unique `id`.
- Have explicit `blockedBy` dependencies.
- Be small enough for one implementation agent and one primary `assignedSkill`.
- Include concrete scope and out-of-scope statements.
- Include acceptance criteria and test scenarios.
- Include verification policy with `minimumScore` and `requiredEvidence`.
- Include `research` in `requiredEvidence` when current-date claims, model/dependency freshness, or external discovery influence the ticket.
- Include `modelPolicy` when the ticket needs a specific host, maximum quality, lower token cost, or policy-approved web research.
- Avoid foundation-only work that is not tied to a current requirement.

## Plan Workflow

1. Read the skill inventory and requirements.
2. Split work into executable tickets.
3. Assign each ticket to a real skill name from the inventory.
4. Set plan-level `modelPolicy` and ticket-level overrides only where needed.
5. Define `blockedBy` relationships.
6. Produce canonical plan JSON.
7. Run:

```bash
orquestra plan validate <plan.json>
orquestra plan waves <plan.json>
orquestra plan explain <plan.json>
orquestra model recommend --ticket <plan.json> --host <host> --ticket-id <ticket-id>
orquestra research brief --ticket <plan.json> --host <host> --ticket-id <ticket-id>
```

8. Fix the plan until validation succeeds and model recommendations fit the ticket risk.

## Runtime Notes

- Waves are derived from `blockedBy`; do not hand-maintain wave numbers as source of truth.
- Model recommendations are derived at dispatch/recommend time; do not hardcode `modelRecommendation` in a hand-authored plan unless importing audited state.
- If no skill matches a ticket, leave the gap explicit and route it through `orquestra-router`.
- Do not use `ORQUESTRA_BOARD.md` as the runtime database. Session state lives under `.orquestra/sessions/`.
