# Stackbench v2 - ROADMAP

Date: 2026-03-16
Status: Active
Depends on: `STACKBENCH_V2_AGENTS.md`, `STACKBENCH_V2_ARCHITECTURE.md`, `STACKBENCH_V2_REPO_LAYOUT.md`, `STACKBENCH_V2_GAP_CLOSURE.md`, `STACKBENCH_V2_DESKTOP_PLAN.md`

## Purpose
Sequence delivery for the local runtime, desktop workbench, and the next cutover steps toward the `stackbench` repo.

## Planning Rules
- Keep canonical state ownership ahead of new surfaces.
- Keep machine-readable runtime contracts ahead of richer GUI features.
- Keep local packaging and auth flows ahead of persona/profile polish.
- Keep `jj` integration discipline ahead of PR automation.
- Keep adapters behind one normalized contract.

## Current Implemented Slice
As of 2026-03-16, the repo already includes:
- `swb run start`, `swb run status`, `swb run list`, and `swb run logs`
- `swb launcher run-once` and `swb launcher watch`
- SQLite-backed durable enqueue and replay-safe projection
- persisted timeline history in canonical state
- adapter auth status and login flows through CLI and desktop
- adapter execution, repository evaluation, approval, rejection, and `jj` integration
- Electron workbench for repo selection, auth, run dispatch, logs, watch mode, and review actions
- desktop smoke tests that run against a prebuilt `swb` binary
- local packaging verified with `pnpm --dir desktop package`

## Phase 0 - Local Runtime And Workbench
Goal: ship a coherent local product baseline that is ready to become the first `stackbench` repo.

Deliver:
- `swb` CLI as the operator surface
- `launcher`, `receiver`, `projector`, and SQLite ingest queue
- canonical run logs and status inspection
- normalized adapter contract
- adapter auth status and login initiation
- `jj` workspace isolation and integration
- repository evaluation gating
- Electron workbench over the same runtime contracts

Exit criteria:
- `swb run start` returns after durable enqueue
- `swb run logs` reads canonical applied history
- desktop can dispatch, inspect, and review runs
- evaluation gates the run before review
- approved runs integrate through `jj`
- desktop smoke tests and local packaging pass

## Immediate Follow-On
1. validate `pnpm --dir desktop make` on a Debian host and record the result
2. bundle a production `swb` binary into packaged desktop builds
3. improve remediation for adapter login flows that require an external terminal
4. add filtered and paginated log views before expanding the surface area

## Phase 1 - Operator Depth
Goal: deepen the local operator experience without changing the architecture.

Add:
- richer log filtering and export
- clearer desktop watch lifecycle and recovery UX
- persona and profile selection
- `gstack` preview and resolution visibility
- stronger adapter doctor output
- packaging notes and install verification for supported hosts

Rules:
- desktop still remains a shell over the same runtime
- new views do not invent a second state model
- persona/profile/gstack features reuse the same config and canonical IDs

## Phase 2 - Optional Networked Surfaces
Goal: add optional remote-facing control surfaces only after the local baseline is solid.

Add:
- local HTTP/API receiver if needed
- remote launcher support where justified
- external ingress adapters such as Slack or Linear
- export and PR automation downstream of approved canonical state

Rules:
- external systems remain non-canonical
- networked ingress does not bypass the ingest queue semantics
- scaling does not change launcher-only canonical writes

## What This Repo Intentionally Leaves Behind
- legacy browser-first orchestration
- tmux-first execution control
- monolithic server-era runtime structure
- direct adapter writes into task or run state

## Success Condition
Stackbench v2 is successful when this repo can be lifted into a new `stackbench` repository with minimal churn because the local runtime, desktop workbench, screenshot, README, and operator docs already describe one coherent product.
