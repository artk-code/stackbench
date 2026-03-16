# Stackbench

*A local workbench for running, reviewing, and integrating agent work.*

Stackbench is a local workbench for running coding agents against a repository, inspecting a canonical event timeline, and integrating only the changes a human approves.

![Stackbench workbench on macOS](docs/stackbench-workbench-macos.png)

## Status Snapshot
- Desktop workbench: running and smoke-tested on macOS
- Local runtime: queue, logs, approval, and integration loop working
- Adapter auth: Codex flow wired, generic adapter auth contract in place
- Worker types: markdown-backed profiles resolve into a minimal gstack and are editable from the desktop shell
- Ingress: local Slack and Linear endpoints resolve personas into queued runs through the same canonical path
- Packaging: Electron packaging verified locally; Linux `.deb` validation is next
- Not shipped yet: bundled production `swb` binary, outbound webhook delivery, Slack approvals, Linear sync, richer login remediation for external TTY flows

## What Stackbench Does
- dispatches local agent runs against a repo
- shows adapter readiness and login state before work starts
- records a canonical timeline for each run
- resolves markdown-backed worker types into reusable run behavior
- resolves ingress-facing personas into the same profile and gstack model
- accepts Slack and Linear ingress without letting them own execution state
- gates approval and integration through a human operator
- keeps the local operator loop usable from a desktop shell instead of a pile of terminals

## Why It Exists
Most agent tooling makes execution easy and review messy.

Stackbench takes the opposite position: runs should stay legible enough that a human can inspect the trail, understand the state, and decide what gets integrated.

## Current Product Shape
- Rust core for queueing, canonical state, evaluation, approval, and `jj` integration
- local HTTP ingress for Slack slash/actions and Linear issue/comment webhooks
- Electron workbench for dispatch, auth checks, logs, watch mode, and review actions
- SQLite-backed ingest queue and canonical state store under `.swb/`
- repo-local runtime assets under `swb/` for worker types, personas, and prompt layers
- Playwright desktop smoke tests that launch Electron against a prebuilt `swb` binary

## Quickstart
```bash
pnpm install
pnpm desktop:build
pnpm desktop:dev
```

Useful verification commands:
```bash
pnpm desktop:lint
pnpm desktop:test:e2e
pnpm desktop:capture:screenshot
pnpm --dir desktop package
```

## Project Status
### Working now
- dispatch a run from the GUI
- create and edit markdown-backed worker types from the GUI
- inspect canonical logs from the GUI
- start and stop launcher watch
- approve, reject, and integrate from the GUI
- queue work from Slack and Linear into the same local runtime
- inspect and manage repo-local personas from the CLI
- package the desktop app locally on macOS

### Next up
- validate `.deb` output on a Debian or Ubuntu host
- bundle a production `swb` binary into packaged builds
- add a real outbound sender for queued Slack and Linear status updates
- handle login flows that require a real external terminal more gracefully
- add Slack approval actions, Linear comment sync, and lease fencing after outbound delivery is reliable

## Design Principles
- local-first execution
- human review before integration
- canonical state owned by the product, not by adapters
- machine-readable contracts between runtime and GUI
- selective reuse of good infrastructure, not loyalty to old architecture

## Core Commands
```bash
swb run start
swb run status
swb run list
swb run logs
swb run approve
swb run reject
swb run integrate
swb profile list
swb profile show
swb profile save
swb persona list
swb persona show
swb persona save
swb launcher run-once
swb launcher watch
swb ingress serve
swb outbound list
swb adapter auth status
swb adapter auth login
```

## Repo Shape
- `crates/` for the Rust runtime and CLI
- `desktop/` for the Electron workbench
- `docs/` for screenshots, plans, and operator notes
- `swb/` for repo-local worker types, personas, and prompt assets

## Supporting Docs
- [STACKBENCH_ARCHITECTURE.md](STACKBENCH_ARCHITECTURE.md)
- [STACKBENCH_REPO_LAYOUT.md](STACKBENCH_REPO_LAYOUT.md)
- [STACKBENCH_ROADMAP.md](STACKBENCH_ROADMAP.md)
- [STACKBENCH_DESKTOP_PLAN.md](STACKBENCH_DESKTOP_PLAN.md)
- [STACKBENCH_ADAPTER_CONTRACT.md](STACKBENCH_ADAPTER_CONTRACT.md)
- [STACKBENCH_CANONICAL_STATE.md](STACKBENCH_CANONICAL_STATE.md)
- [STACKBENCH_INGRESS_SPEC.md](STACKBENCH_INGRESS_SPEC.md)
