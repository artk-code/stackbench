# Stackbench Agent Specification

Date: 2026-03-16  
Status: Active

## Objective
Build Stackbench as a local workbench for running coding agents against a repository, inspecting a canonical timeline, and integrating only the changes a human approves.

## Active Docs
- `README.md`
- `STACKBENCH_AGENTS.md`
- `STACKBENCH_ARCHITECTURE.md`
- `STACKBENCH_REPO_LAYOUT.md`
- `STACKBENCH_ROADMAP.md`
- `STACKBENCH_DESKTOP_PLAN.md`
- `STACKBENCH_CANONICAL_STATE.md`
- `STACKBENCH_ADAPTER_CONTRACT.md`
- `STACKBENCH_INGRESS_SPEC.md`
- `STACKBENCH_GSTACK_SPEC.md`
- `STACKBENCH_PERSONA_PROFILE_MAPPING.md`
- `STACKBENCH_EVAL_LEASE_RUNTIME.md`
- `STACKBENCH_CUTOVER_CHECKLIST.md`

## Repo Scope
This repo is intentionally trimmed to the runtime and desktop workbench.

Included:
- Rust runtime crates under `crates/`
- `desktop/` Electron workbench
- `scripts/swb-jj.sh`
- current design and contract docs
- current product README and screenshots

Excluded:
- legacy tmux/browser orchestration
- legacy Phase 0 sign-off docs
- archived planning bundles
- the old `web/` control surface

## Current Product Baseline
- SQLite-backed ingest queue and canonical state store are active.
- `swb run start|status|list|logs` are implemented.
- `swb profile list|show|save` are implemented.
- `swb persona list|show|save` are implemented.
- `swb launcher run-once|watch` are implemented.
- `swb ingress serve` and `swb outbound list|mark` are implemented.
- adapter auth status and login commands are available through the CLI and desktop shell.
- evaluation, approval, rejection, and `jj` integration are wired into the local runtime.
- markdown-backed worker types resolve through a minimal gstack path and are recorded on runs.
- ingress-facing personas resolve through repo-local TOML files under `swb/personas/`.
- local Slack and Linear ingress enqueue work through the same queue -> receiver -> state path.
- Electron workbench supports repo selection, worker-type editing, run dispatch, watch control, auth checks, logs, and review actions.
- Desktop smoke tests run against a prebuilt `swb` binary rather than `cargo run`.

## Core Contracts
- Stackbench owns canonical state.
- Adapters never write canonical state directly.
- The launcher is the only writer into the ingest path.
- Accepted envelopes flow through queue -> receiver -> projector -> canonical state.
- Slack and Linear ingress are additive request surfaces, not secondary state owners.
- Desktop is an operator shell over the same machine-readable contracts; it is not a second orchestrator.

## Operator Surface
- `swb run start`
- `swb run status`
- `swb run list`
- `swb run logs`
- `swb run approve`
- `swb run reject`
- `swb run integrate`
- `swb persona list`
- `swb persona show`
- `swb persona save`
- `swb launcher run-once`
- `swb launcher watch`
- `swb ingress serve`
- `swb outbound list`
- `swb outbound mark`
- `swb adapter list`
- `swb adapter doctor`
- `swb adapter auth status`
- `swb adapter auth login`

## Current Priorities
1. Keep the local runtime and desktop workbench aligned with the docs.
2. Validate Linux packaging on a Debian host.
3. Bundle a production `swb` binary into packaged desktop builds.
4. Add outbound delivery for queued Slack and Linear status updates.
5. Improve auth remediation for adapters that need an external TTY.
6. Add Slack approval actions, Linear sync, and ingress lease fencing only after outbound delivery is stable.

## Verification Commands
- `cargo test -p swb-core -p swb-config -p swb-queue-sqlite -p swb-receiver -p swb-state -p swb-eval -p swb-jj -p swb-launcher -p swb-cli -p swb-adapters`
- `cargo test -p swb-ingress-http`
- `pnpm desktop:lint`
- `pnpm desktop:test:e2e`
- `pnpm desktop:capture:screenshot`
- `pnpm --dir desktop package`

## Working Rules
- Do not reintroduce `web/` as a primary control surface.
- Do not add new state mutation paths that bypass the ingest queue.
- Do not let Slack or Linear become a second lifecycle or approval source of truth.
- Do not let desktop parse human-only output when a machine-readable path exists.
- Keep docs current when behavior or repo shape changes.
- Keep internal runtime naming consistent with `swb` and keep product-facing copy consistent with `Stackbench`.
