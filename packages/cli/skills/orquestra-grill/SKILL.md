---
name: orquestra-grill
description: Conducts Orquestra's adaptive discovery interview. It inspects local facts before asking, asks one question at a time, and ties each question to the confidence gap that needs evidence.
version: 1.0.0
---

# Orquestra Grill

Use this official skill for a new project or when no document review is required.

## Interview Contract

The interview improves an Orquestra init session until it has enough evidence to plan. Do not depend on `grill-me` or any external interview skill.

Before every question:

1. Inspect available local facts read-only: project files, manifests, existing `.orquestra/` state, and prior answers.
2. Do not ask for a fact already available locally. State the fact in the working context and use it to narrow the next question.
3. Identify the single confidence dimension with the largest unresolved gap: problem understanding, requirements, constraints, stakeholders, success criteria, risk, or technical context.
4. Record the selected dimension as the question's confidence gap in the init session or host working context.

Ask exactly one concise, answerable question. The question must request evidence that improves its selected confidence gap. Do not combine unrelated questions or present a questionnaire.

After an answer:

1. Persist it with `orquestra init answer` and extract durable requirements with `orquestra init add-requirement`.
2. Reassess the available facts and confidence gaps before asking again.
3. Begin research only after the local facts and user answers identify a researchable uncertainty.

## Question Selection

Use the smallest question that resolves the gap. Prefer concrete choices when local facts establish the options. Examples:

- Problem understanding: ask which user outcome matters most when the stated goal is broad.
- Requirements: ask which workflows are mandatory for the first release when the audience is known.
- Constraints: ask for the required runtime, deployment target, or compatibility boundary when the repository does not establish it.
- Success criteria: ask for a measurable acceptance condition when a desired outcome has no testable definition.
- Risk: ask for compliance, security, or rollback constraints when the domain indicates them but evidence is missing.

Do not ask for implementation details merely to fill time. Stop questioning a dimension once available evidence is sufficient, and move to the next material gap.

## Guardrails

- Keep all local inspection read-only during discovery.
- Never invent repository facts, user answers, sources, or confidence gains.
- Do not read document contents through this skill; use `orquestra-grill-with-docs` when documentary context is needed.
- Do not plan while a required confidence gap remains unresolved.
- If progress stalls, explain the specific missing evidence and ask the one question that can unblock it.
