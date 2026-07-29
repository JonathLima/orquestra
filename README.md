<p align="center">
  <img src="./assets/orquestra-logo.png" alt="Orquestra" width="960">
</p>

# Orquestra

[Source](https://github.com/JonathLima/orquestra) |
[npm](https://www.npmjs.com/package/@jonathlima/orquestra) |
[CI](https://github.com/JonathLima/orquestra/actions/workflows/ci.yml) |
[Releases](https://github.com/JonathLima/orquestra/releases) |
[WIE MCP](https://github.com/JonathLima/WIE_MCP) |
[AGPL-3.0-only](./LICENSE)

Orquestra is a local-first orchestration extension for AI coding CLIs. You give
your preferred CLI a problem through `orquestra-init`; Orquestra then drives the
discovery, web research, planning, skill selection, execution, rerouting, and
verification cycle.

It is not another AI model and does not replace Codex, Claude Code, OpenCode, or
Antigravity. The selected host performs the reasoning and uses its configured
web-search/MCP tools. Orquestra supplies the workflow and a native Rust runtime
that validates state, confidence, plans, dependencies, evidence, and safety
rules.

## Why Orquestra

Coding agents can begin implementing before they understand the actual demand,
use weak research, select unrelated skills, or declare success without evidence.
Orquestra adds a repeatable product workflow:

1. Inspect the local project without asking for facts already available.
2. Ask one focused question at a time to resolve the largest confidence gap.
3. Research technical uncertainty through the host's configured web-search MCP.
4. Cross-check sources and converge on a minimum confidence threshold.
5. Create a demand-specific action plan and deterministic execution DAG.
6. Select only relevant installed skills and quarantine project adaptations.
7. Execute in waves, reroute failed work, and require evidence before completion.

## Requirements

- Node.js 20 or newer, or Bun
- One supported AI coding CLI:
  - Codex
  - Claude Code
  - OpenCode
  - Antigravity
- A web-search tool configured in that host when the demand requires research

Orquestra does not require a specific search provider. A host can use
[WIE MCP](https://github.com/JonathLima/WIE_MCP) or any other search connector
mapped in the Orquestra configuration.

## Install

Install the npm wrapper globally:

```bash
npm install --global @jonathlima/orquestra
```

Or use Bun:

```bash
bun add --global @jonathlima/orquestra
```

Run a command without installing globally:

```bash
npx @jonathlima/orquestra doctor --security
npx @jonathlima/orquestra setup --host opencode
```

The wrapper selects the native package for the current operating system and CPU.

Confirm the installation:

```bash
orquestra doctor --security
```

## Configure A Host

Register Orquestra once in each AI CLI you want to use:

```bash
orquestra setup --host codex
orquestra setup --host claude-code
orquestra setup --host opencode
orquestra setup --host antigravity
```

Use `--dry-run` to inspect every file operation before setup writes anything:

```bash
orquestra setup --host opencode --dry-run
```

Setup installs seven portable Agent Skills and the host-specific
`orquestra-init` entry point. It does not install an AI model or replace the
host's configuration.

## Start A Request

After setup, open the AI CLI in your project and provide the complete problem
after `orquestra-init`.

### Codex

```text
$orquestra-init <problem>
```

### Claude Code

```text
/orquestra-init <problem>
```

### OpenCode

```text
/orquestra-init <problem>
```

### Antigravity

Invoke the installed `orquestra-init` skill with `<problem>`.

Example:

```text
/orquestra-init Migrate this API to the current Node.js LTS without breaking
existing clients. Research compatibility risks, define rollback criteria, and
produce an implementation plan before changing code.
```

The user starts the request once. The host then follows the Orquestra protocol
and asks for input only when local inspection or research cannot resolve an
important decision.

## What Happens Automatically

### 1. Discovery

Orquestra classifies the demand, inspects local project facts, and uses
`orquestra-grill` or `orquestra-grill-with-docs` to ask focused questions.
Document contents are read only after explicit, consolidated consent.

### 2. Web Research

When evidence is needed, the Rust runtime emits a research delegation envelope.
The AI host calls its own mapped web-search/MCP tool and returns the result to
Orquestra.

Research reports are validated before they can increase confidence:

- five-source cross-validation by default
- trusted-domain and primary-source preference
- independent-domain checks
- SHA-256 content hashes
- private-network and unsafe-address rejection
- project-local storage and audit history

Web content is always treated as untrusted input, never as executable
instructions.

### 3. Confidence Convergence

The default minimum confidence is `0.95`. Understanding, answered questions,
requirements, contradictions, blockers, and validated research all contribute
to the convergence gate. A high average cannot hide a weak required component.

Projects can adjust the threshold in `.orquestra/config.toml`:

```toml
[init]
min_confidence = 0.95
```

Planning and application remain blocked until the configured gate is satisfied.

### 4. Adaptive Planning

After convergence, the host creates a plan for the actual demand rather than
using a fixed stack template. The runtime validates:

- requirement coverage
- ticket acceptance criteria
- real dependency references
- acyclic execution order
- verification and research policy
- deterministic execution waves

### 5. Skill Routing And BRAIN

Orquestra scans the user's available Agent Skills and selects only skills that
make sense for each ticket. It never modifies global skills.

If no safe match exists, the workflow reports a skill gap. Policy-approved
external discovery can use `find-skills`; local adaptation is created under
`.orquestra/skills/_pending/` for review. Approved adaptations are scoped to the
current project, stack, and demand.

### 6. Execution And Verification

Tickets are dispatched in dependency-safe waves. Each attempt receives a stable
manifest. Failed verification can reroute the ticket with new context instead of
silently accepting the result.

A ticket cannot complete without a persisted report that matches its current
attempt, assigned skill, score policy, and required evidence. Wave transitions
require explicit checkpoints.

## Bundled Skills

| Skill | Purpose |
| --- | --- |
| `orquestra-init` | Public entry point and autonomous discovery loop |
| `orquestra-grill` | Focused interview for new demands |
| `orquestra-grill-with-docs` | Consent-based interview using project documents |
| `orquestra-orchestrator` | End-to-end workflow coordination |
| `orquestra-planner` | Demand-specific DAG planning |
| `orquestra-router` | Installed-skill matching and gap handling |
| `orquestra-verifier` | Evidence-based result verification and rerouting |

## Local Project State

Orquestra stores runtime data inside the current project:

```text
.orquestra/
  config.toml
  skills_inventory.json
  skills/
    _pending/
  sessions/
  verification/
  research/
  memory/
```

This state is local by default and should not be committed. Session events,
research hashes, plans, dispatch attempts, and verification reports remain
available for inspection.

## Useful Commands

Normal users primarily need `setup`, `doctor`, and the host's
`orquestra-init` command.

```bash
# Diagnose installation, hosts, policy, and write roots
orquestra doctor --security

# Check the npm package version
orquestra update check

# Preview or repeat host setup
orquestra setup --host opencode --dry-run
orquestra setup --host opencode

# Inspect installed skills
orquestra skill scan
orquestra skill list

# Inspect init sessions when troubleshooting
orquestra init list
orquestra init status --session-id <session-id>

# Inspect runtime sessions and evidence
orquestra session list
orquestra session show <session-id>
orquestra verify report <session-id>
```

Run `orquestra <command> --help` for the complete command interface.

## Security Model

- The Rust runtime is authoritative for validation and state transitions.
- Verification commands use literal argument arrays, never shell strings.
- Verification profiles enforce timeouts, output caps, and artifact checks.
- Secrets are redacted from persisted process output.
- Global user skills are read-only except when the user explicitly authorizes
  setup or synchronization of Orquestra-owned skills.
- BRAIN candidates are quarantined until approved.
- External skill discovery is disabled unless policy explicitly enables it.
- Research rejects private-network destinations and records source hashes.
- Project writes are constrained to configured roots.

Review the active policy at any time:

```bash
orquestra doctor --security
orquestra brain policy
```

## Supported Platforms

| Operating system | Architecture |
| --- | --- |
| Windows | x64 |
| macOS | x64, ARM64 |
| Linux | x64, ARM64 |

The universal `@jonathlima/orquestra` wrapper uses npm optional dependencies to
select the matching native binary.

## Current Validation Status

The local consumer flow has been validated on Windows x64 using an npm tarball,
OpenCode, and WIE-compatible MCP web search. The validation covered installation,
seven-skill setup, discovery, five-source research, confidence convergence,
adaptive planning, skill gaps, BRAIN quarantine, DAG waves, failed verification,
rerouting, checkpoints, and final evidence reports.

The Rust workspace currently has 382 passing tests across 30 suites. Full
registry publication, release signatures, and the complete operating-system/CPU
installation matrix must still pass the release workflow before a stable public
tag.

## Development

Build and validate from the repository:

```bash
git clone git@github.com:JonathLima/orquestra.git
cd orquestra

cargo fmt --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets

cd packages/cli
npm run pack:local
node index.cjs doctor --security
```

The main components are:

```text
crates/
  orquestra-cli/       CLI commands and bundled skills
  orquestra-core/      configuration and security primitives
  orquestra-adapters/  Codex, Claude Code, OpenCode, and Antigravity adapters
  orquestra-config/    versioned configuration schema
  orquestra-init/      discovery, research, and convergence
  orquestra-skills/    inventory, routing, and BRAIN
  orquestra-plan/      canonical plans and DAG waves
  orquestra-runtime/   durable execution and verification state

packages/
  cli/                 universal npm wrapper and public skills
  cli-platform-*/      native platform packages
```

Release history and comparisons between versions are recorded in
[GitHub Releases](https://github.com/JonathLima/orquestra/releases).

## Maintainer Release

Pushing `main` runs the complete CI workflow but does not publish to npm.
Publishing is triggered only by a version tag.

Before the first release, create a granular npm token with read/write access and
2FA bypass for the `jonathlima` scope, then add it to the GitHub repository as
the Actions secret `NPM_TOKEN`. Make sure the tag matches the version in all npm
manifests:

```bash
git push origin main
git tag v0.1.1
git push origin v0.1.1
```

The release workflow validates the token owner, runs the complete test suite,
builds and signs all five native packages, publishes them, publishes the
universal wrapper, and then creates a GitHub Release with generated comparison
notes. A failed release can be rerun safely; packages and release entries
already present at the same version are skipped.

After the workflow succeeds:

```bash
npx @jonathlima/orquestra doctor --security
```

## License

Copyright (C) 2026 Jonathan Lima.

Orquestra is licensed under the
[GNU Affero General Public License v3.0 only](./LICENSE).

If you distribute Orquestra, a modified version, or covered object code, you
must make the Corresponding Source available under the AGPLv3 terms. A modified
version that supports remote network interaction must prominently offer its
Corresponding Source to users interacting with it over that network.

The AGPLv3 permits commercial use. It does not require unpublished private or
internal modifications to be posted publicly.
