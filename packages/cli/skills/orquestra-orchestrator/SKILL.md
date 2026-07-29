---
name: orquestra-orchestrator
description: Internal runtime support for an applied Orquestra plan. Executes DAG waves, delegates tickets to approved skills, enforces evidence-based verification, reroutes failures, and manages wave checkpoints.
version: 0.8.0
---

# Orquestra Orchestrator

`orquestra-init` is the public entry point. If a user invokes this skill directly
with a new problem, load `orquestra-init` and preserve the original arguments.

When forwarding discovery, preserve these public rules without running a second
discovery loop:

- Select `orquestra-grill` for a new project or when no document content is authorized.
- Select `orquestra-grill-with-docs` for an existing project.
- Obtain consolidated consent for candidate paths and file types before reading any document content.
- Let `orquestra-init` enforce the default `0.95` confidence gate and five sources
  per research topic.

Use this skill after `orquestra init apply` returns a runtime session ID.

## Runtime Authority

- Read canonical state through Orquestra commands; never edit session JSON.
- Execute only the current wave.
- Use the effective skill written in each ticket manifest.
- Pending BRAIN candidates are not executable skills.
- Ticket completion requires a persisted passing verification report for the
  current dispatch attempt and effective skill.
- A completed wave advances only after explicit checkpoint approval.

## Execute A Wave

```bash
orquestra run start <runtime-id>
orquestra run dispatch <runtime-id> --host <host>
```

For every returned manifest:

1. Read objective, acceptance criteria, model recommendation, verification
   policy, dispatch attempt ID, and effective assigned skill.
2. Load that approved skill from the current inventory.
3. Follow the skill's inherited domain guidance while adapting commands and APIs
   to project manifests, lockfiles, stack, and versions.
4. Dispatch independent tickets in parallel when the host supports subagents.
5. Collect concrete artifacts and run only checks that prove the acceptance
   criteria.
6. Delegate independent evaluation to `orquestra-verifier`.
7. Write the report under `.orquestra/` and enforce it:

```bash
orquestra verify ticket --report <report.json> --plan <session-plan.json>
orquestra run complete-ticket <runtime-id> <ticket-id> --output "<summary>" --evidence "<artifact>"
```

The report must repeat the manifest's exact `sessionId`, `ticketId`,
`dispatchAttemptId`, and `assignedSkill`. Never grant a high score without
evidence. Include every evidence kind required by the ticket policy.

## Research-Required Tickets

If the model recommendation has `webRequired: true`, dispatch research through
the host's configured MCP/native web search capability. Cross-check current-date
claims and store a valid runtime research report before completion. Never invent
sources or perform direct HTTP inside Orquestra.

## Failure And Real Rerouting

On verification failure:

```bash
orquestra run fail-ticket <runtime-id> <ticket-id> --output "<failure>"
orquestra skill match --ticket <ticket.json>
```

Select the highest relevant active alternative other than the failed effective
skill. Then run:

```bash
orquestra run reroute-ticket <runtime-id> <ticket-id> --reason "<correction>" --skill <alternative>
```

If no relevant alternative exists, omit `--skill` and retry the same route with
the verifier's specific correction. Dispatch the current wave again. The runtime
uses a new attempt ID and rejects stale verification reports. Respect the bounded
retry limit.

## Checkpoints

When every ticket in the wave passes, summarize delivered artifacts and request
one explicit approval:

```bash
orquestra run approve-wave <runtime-id> --wave <wave-number> --notes "<approval>"
```

Continue until:

```bash
orquestra run status <runtime-id>
```

reports `Completed`.

## Invalid Behavior

- Inventing skill names or using inactive inventory entries.
- Using `_pending` adaptations.
- Skipping the assigned skill.
- Completing a ticket after failed verification.
- Reusing a report from an older dispatch attempt.
- Dispatching a future wave before checkpoint approval.
- Ignoring required current-date research.
- Reporting the plan as though it were the final result.
