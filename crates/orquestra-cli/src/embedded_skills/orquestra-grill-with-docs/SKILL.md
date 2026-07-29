---
name: orquestra-grill-with-docs
description: Conducts Orquestra's adaptive discovery interview with consented document review. It inspects local facts first, obtains one consolidated document consent, and ties each question to a confidence gap.
version: 1.0.0
---

# Orquestra Grill With Docs

Use this official skill for an existing project when repository documents can improve the discovery interview.

## Interview Contract

The interview improves an Orquestra init session until it has enough evidence to plan. Do not depend on `grill-me`, `grill-me-with-docs`, or any external interview skill.

Before every question:

1. Inspect available local facts read-only: repository structure, manifests, existing `.orquestra/` state, filenames, document paths, and prior answers.
2. Do not open document contents until document consent is granted.
3. Do not ask for a fact already available locally. Use discovered facts to narrow the next question.
4. Identify the single confidence dimension with the largest unresolved gap: problem understanding, requirements, constraints, stakeholders, success criteria, risk, technical context, or document context.
5. Record the selected dimension as the question's confidence gap in the init session or host working context.

Ask exactly one concise, answerable question. The question must request evidence that improves its selected confidence gap. Do not combine unrelated questions or present a questionnaire.

## Document Consent

When document context would materially reduce a confidence gap:

1. Inventory candidate document paths and file types without reading their contents.
2. Exclude sensitive candidates by default, including `.env*`, credentials, keys, certificates, tokens, secrets, private configuration, and files under directories named `secret`, `secrets`, `private`, or `credentials`.
3. Ask one consolidated consent question that lists the candidate paths or path groups, file types, and the proposed exclusions. Allow the user to approve, deny, or exclude additional paths/types in the same answer.
4. Read only the approved documents. Never infer consent from project ownership, a prior answer, or the existence of a file.
5. Record the granted scope and exclusions before using document content as evidence.

After an answer or approved document review:

1. Persist user answers with `orquestra init answer` and extract durable requirements with `orquestra init add-requirement`.
2. Summarize approved document facts without exposing excluded or sensitive content.
3. Reassess the available facts and confidence gaps before asking again.
4. Begin research only after the local facts, approved documents, and user answers identify a researchable uncertainty.

## Guardrails

- Keep all inspection read-only during discovery.
- Never read or transmit excluded documents or sensitive content.
- Never invent repository facts, user answers, sources, consent, or confidence gains.
- Do not plan while a required confidence gap remains unresolved.
- If progress stalls, explain the specific missing evidence and ask the one question that can unblock it.
