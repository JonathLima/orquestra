# @jonathlima/orquestra

Orquestra is a local-first orchestration harness for AI coding CLIs. The npm
package installs the native runtime and portable Agent Skills; the selected AI
host performs the reasoning while the runtime enforces state, confidence,
routing, DAG execution, checkpoints, and evidence-based verification.

## Install

```bash
npm install --global @jonathlima/orquestra
```

Or run it without a global installation:

```bash
npx @jonathlima/orquestra doctor --security
npx @jonathlima/orquestra setup --host opencode
```

Register Orquestra once in each host you use:

```bash
orquestra setup --host codex
orquestra setup --host claude-code
orquestra setup --host opencode
orquestra setup --host antigravity
```

Use `--dry-run` to inspect every setup change before it is written.

## Start A Request

After the one-time host setup, `orquestra-init` is the public entry point. Add
the complete problem after the command:

```text
Codex:       $orquestra-init <problem>
Claude Code: /orquestra-init <problem>
OpenCode:    /orquestra-init <problem>
Antigravity: invoke the orquestra-init skill with <problem>
```

The host then drives the complete flow: focused grilling, mapped web research,
cross-source validation, confidence convergence, demand-specific planning,
inventory-based skill routing, project-local BRAIN adaptation, DAG dispatch,
checkpoint approval, rerouting after failed attempts, and strict verification.

The default convergence threshold is `0.95`. Projects can adjust it in
`.orquestra/config.toml` under `[init] min_confidence` within the runtime's
allowed range.

## Product Checks

```bash
orquestra doctor --security
orquestra update check
orquestra skill scan
orquestra brain policy
```

Project state and audit evidence are stored under `.orquestra/`. Installed
global skills are read-only; adaptations are created only inside the current
project and require review before activation.

## Supported Platforms

Prebuilt packages are selected automatically by npm for Windows x64, macOS
x64/ARM64, and Linux x64/ARM64.

Version history and comparisons are published in
[GitHub Releases](https://github.com/JonathLima/orquestra/releases).

## License

Copyright (C) 2026 Jonathan Lima.

Orquestra is licensed under the GNU Affero General Public License v3.0 only.
Distribution of this package or its native binaries must be accompanied by
equivalent access to the Corresponding Source under the AGPLv3 terms. Modified
versions used for remote network interaction must offer their Corresponding
Source to the users interacting with them.
