---
name: orquestra-verifier
description: Use when evaluating completed Orquestra ticket output against the assigned skill, ticket acceptance criteria, evidence policy, and runtime verification gate.
version: 0.4.0
---

# Orquestra Verifier

## Overview

Produce evidence-based verification reports that the Rust runtime can enforce. The verifier evaluates quality, but the runtime persists and gates the pass/fail decision.

## Verification Report

Create a JSON report with:

```json
{
  "sessionId": "<session-id>",
  "ticketId": "T1",
  "dispatchAttemptId": "<dispatch-attempt-id>",
  "skillName": "<assigned-skill>",
  "score": 0.97,
  "summary": "Implementation satisfies acceptance criteria and tests passed.",
  "evidence": [
    {
      "kind": "test",
      "description": "cargo test --workspace --all-targets passed",
      "path": null
    }
  ]
}
```

The report must match the session ticket's assigned skill. Evidence descriptions and paths are redacted before persistence.

## Workflow

1. Read the ticket manifest under `.orquestra/sessions/<session-id>/tickets/`.
2. Read the persisted `modelRecommendation`.
3. If `webRequired` is true, read `.orquestra/research/<session-id>/<ticket-id>.json`.
4. Read the assigned skill content from the approved skill source.
5. Read implementation output and evidence.
6. Evaluate against:
   - Ticket acceptance criteria.
   - Required evidence.
   - Assigned skill instructions.
   - Model recommendation risk, tier, reasoning effort, and `webRequired` flag.
   - Validated research claims, source count, primary source, conflicts, and current date.
   - Project test/build/lint expectations.
   - Current-date requirements when BRAIN or external research was involved.
7. Write a verification report JSON.
8. Run:

```bash
orquestra verify ticket --report <report.json> --plan <plan.json>
orquestra run complete-ticket <session-id> <ticket-id>
```

## Scoring

- `score` must be finite and between `0.0` and `1.0`.
- Default pass threshold is ticket policy, typically `0.95`.
- Missing required evidence fails even with a high score.
- Wrong `sessionId`, `ticketId`, `dispatchAttemptId`, or `skillName` fails.
- `webRequired: true` without `orquestra research validate` + stored report fails.
- Single-source web claims, missing primary sources, stale retrieval dates, and unresolved conflicts fail.
- High-risk tickets routed to frontier/high reasoning require stronger evidence before scoring above the pass threshold.

## Reroute

If verification fails, produce a failure report with:

- Violated skill rules.
- Unmet acceptance criteria.
- Missing evidence.
- Missing current-date model or web-research evidence.
- Specific correction directive.

Return the failure report to the orchestrator for reroute instead of marking the ticket complete.
