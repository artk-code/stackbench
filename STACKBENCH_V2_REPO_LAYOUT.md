# Stackbench v2 - REPO LAYOUT

Date: 2026-03-16
Status: Active
Depends on: `STACKBENCH_V2_AGENTS.md`, `STACKBENCH_V2_ARCHITECTURE.md`, `STACKBENCH_V2_DESKTOP_PLAN.md`

## Purpose
Describe the current repo shape for the v2 runtime and desktop workbench.

## Workspace Strategy
Stackbench v2 keeps the existing Cargo workspace shell and shared dependency management, but the repo now carries only the runtime and workbench needed for the Stackbench cutover.

Top-level structure:

```text
crates/
desktop/
docs/
scripts/
README.md
STACKBENCH_V2_*.md
STACKBENCH_CUTOVER_CHECKLIST.md
Cargo.toml
package.json
pnpm-workspace.yaml
```

## Active Rust Crates

```text
crates/
  swb-cli/
  swb-core/
  swb-launcher/
  swb-adapters/
  swb-queue-sqlite/
  swb-receiver/
  swb-state/
  swb-eval/
  swb-jj/
  swb-config/
```

## Crate Responsibilities
### `swb-cli`
- operator-facing commands
- machine-readable command output for desktop integration
- run submission, status, list, and log views
- approval and integration actions

### `swb-core`
- shared domain types
- run identifiers
- state transition primitives
- normalized event envelope types

### `swb-launcher`
- queued run execution orchestration
- adapter supervision
- evaluation handoff
- foreground watch mode

### `swb-adapters`
- adapter registry
- adapter capability reporting
- auth doctor and login flows
- normalized adapter result handling

### `swb-queue-sqlite`
- SQLite-backed durable ingest queue
- enqueue, claim, ack, and replay primitives

### `swb-receiver`
- ingest acceptance and validation
- receiver-side application boundary

### `swb-state`
- canonical state database
- replay-safe projection
- run list, status, and log timeline views

### `swb-eval`
- repository-defined evaluation runner
- pass/fail normalization for run gating

### `swb-jj`
- workspace creation
- change and bookmark helpers
- integration helpers built on `jj`

### `swb-config`
- `swb.toml` parsing
- adapter registration
- workflow and policy configuration
- profile, persona, and gstack lookup

## Desktop Package

```text
desktop/
  src/
    main.ts
    preload.ts
    renderer/
  tests/
  playwright.config.ts
```

The desktop package is the only GUI in this repo. It is an operator shell over the local runtime, not a second orchestrator.

## Runtime Assets
Recommended repo-local runtime assets:

```text
swb/
  gstacks/
  profiles/
  personas/
  prompts/
```

These assets are resolved through configuration and launcher logic. They do not replace the Rust workspace.

## Supporting Scripts
Retained script surface:

```text
scripts/
  swb-jj.sh
```

`swb-jj.sh` remains as the `jj` helper and guardrail script for integration-oriented flows.

## Repo Boundary Rules
- `desktop/` consumes machine-readable runtime interfaces only.
- `swb-adapters` does not mutate canonical state directly.
- `swb-queue-sqlite` stays provider-agnostic.
- `swb-state` owns derived reads, not subprocess execution.
- `swb-jj` owns repository workspace mechanics, not run-state policy.
- legacy browser and tmux orchestration code is intentionally absent from this repo shape.

## Configuration Shape
Stackbench v2 assumes a repository-local config file:

```text
swb.toml
```

Current config concerns:
- registered adapters
- workflow definitions
- evaluation commands
- integration policy
- workspace defaults
- persona, profile, and gstack resolution

## Migration Posture
This repo is already in the cutover shape for the next `stackbench` repository:
- runtime crates and desktop workbench are present
- legacy server-era crates are gone
- legacy browser UI is gone
- product docs and screenshot reflect the current desktop shell

The next rename step can change internal `swb` identifiers, but the architectural boundaries in this document should remain stable.
