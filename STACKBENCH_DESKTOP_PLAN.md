# Stackbench - Desktop Plan

Date: 2026-03-16
Status: Draft
Depends on: `STACKBENCH_ARCHITECTURE.md`, `STACKBENCH_ADAPTER_CONTRACT.md`, `STACKBENCH_PERSONA_PROFILE_MAPPING.md`, `STACKBENCH_GAP_CLOSURE.md`

## Purpose
Define the desktop GUI plan for Stackbench so future agents can build the shell without re-litigating architecture, packaging, or auth behavior.

## Decision
Stackbench should use an Electron desktop shell for the first GUI.

Why this is the current direction:
- the repo already has a TypeScript and Vite desktop baseline
- Electron lets Stackbench ship a polished local operator shell quickly
- macOS packaging is already working locally and Linux packaging is a clear next step
- desktop shell quality matters more than strict binary size right now

## Scope
The desktop app is an operator shell around the Rust core.

The desktop app is not:
- a second orchestrator
- a second state machine
- a renderer that shells out directly to CLIs
- a replacement for the canonical ingest path

## Initial Desktop Surface
The first desktop release should cover:
- repository selection
- adapter doctor and auth state
- login initiation when supported
- run start
- run list and run detail
- canonical run logs
- launcher watch start and stop
- approve, reject, and integrate

Later additions may include:
- persona presets
- richer gstack preview
- workspace browsing
- Slack ingress visibility
- external refs and outbound update views

## Architecture
### Core rule
Rust remains the product core.

Electron is only the desktop control surface.

### Process model
#### Main process
- owns all privileged operations
- launches the Rust CLI or packaged swb binary
- manages long-running watch subprocesses
- exposes a narrow IPC surface

#### Preload
- exports a typed bridge into the renderer
- no raw Node or shell access leaks into the renderer

#### Renderer
- displays state and invokes typed desktop actions
- never shells out directly
- never parses human-only terminal text if machine-readable output exists

## Machine Interface Rule
The desktop shell should prefer machine-readable CLI or local API contracts.

Minimum stable machine interface:
- `swb run start --json`
- `swb run status --json`
- `swb run list --json`
- `swb run logs --json`
- `swb persona list --json`
- `swb persona show <PERSONA_ID> --json`
- `swb launcher run-once --json`
- `swb launcher watch --json`
- `swb outbound list --json`
- `swb adapter list --json`
- `swb adapter doctor --json`
- `swb adapter auth status --json`
- `swb adapter auth login --json`

The GUI must not depend on parsing tab-separated human output as its primary integration path.

## Security Rules
- `contextIsolation` must stay enabled
- `nodeIntegration` must stay disabled
- renderer IPC must be allowlisted and typed
- file system and subprocess access stays in main process only
- auth checks and login actions must use the same adapter contract available to terminal users

## Adapter Auth In GUI
The desktop app must surface:
- command availability
- logged-in status when detectable
- auth method when detectable
- whether login is supported
- whether device login is supported
- remediation detail when login cannot be completed

### Login behavior
- if adapter login can complete as a subprocess, desktop may launch it
- if login is interactive or device-based, desktop should display progress and remediation text
- if login requires a real terminal TTY and cannot complete inside the app, desktop should say so explicitly and show the exact command to run outside the app

### Current expectation
- Codex is the first fully supported auth flow
- other adapters may initially support `auth_status` only, or no auth contract at all

## Launcher Watch In GUI
The desktop app should treat launcher watch as a main-process owned long-running job.

Rules:
- one watch process per selected repo root
- watch lifecycle must be visible in the UI
- watch output should stream into a feed view
- closing the app should stop the watch process cleanly unless explicit background mode exists later

## Packaging Rules
### Current desktop packaging
- ship packaged macOS and Linux operator builds
- validate `.deb` on Debian or Ubuntu as the first Linux packaging path
- manual install or normal package-manager install is acceptable

### Known packaging limitation
- Electron auto-update is not a Linux v1 requirement
- Stackbench should not block desktop adoption on Linux auto-update support

### Runtime bundling
Production packaging will need one of these:
1. bundle a compiled `swb` binary alongside the app
2. require a preinstalled `swb` binary and detect it

Preferred direction:
- dev mode may use `cargo run -p swb-cli -- ...`
- packaged mode should bundle a binary rather than requiring Rust on the operator machine

### Packaging note for this repo
Electron Forge packaging under `pnpm` requires a hoisted linker layout. This repo sets `node-linker=hoisted` in `.npmrc` so `pnpm --dir desktop package` works without per-machine pnpm config.

## Automated QA
Current desktop QA commands:
- `pnpm desktop:lint`
- `pnpm desktop:test:e2e`
- `pnpm desktop:capture:screenshot`

The GUI smoke suite builds a precompiled `swb` binary first, launches Electron against that binary, and uses a temporary repo fixture with fake adapter and `jj` commands so the desktop path is exercised without external service dependencies.

## Desktop Data Model
The renderer should treat these as first-class views:
- desktop state
- adapter auth state
- run summary list
- run log timeline
- launcher watch feed
- ingress persona list
- external refs and outbound updates for a selected run
- last error or remediation message

It should not invent its own run lifecycle or approval semantics.

## Recommended Repo Structure
```text
desktop/
  src/
    main.ts
    preload.ts
    renderer/
```

This package should stay separate from:
- Rust crates implementing the core runtime
- any future alternate surface that does not own canonical state

## Known Risks
- packaged binary lookup can diverge from dev-mode CLI invocation
- PATH and environment inheritance differ across Linux desktop launches
- adapter login commands may require a TTY the desktop app cannot provide
- `jj` or adapter CLIs may be missing on operator machines
- long run logs can overwhelm the renderer without pagination or limits
- watch subprocesses can outlive UI state if lifecycle cleanup is sloppy
- letting the desktop shell drift into its own orchestration logic will confuse future contributors unless docs stay explicit

## Open Questions
- should the desktop app eventually call a local HTTP receiver instead of the CLI
- should login actions open an embedded terminal panel or always externalize unsupported interactive flows
- when packaged, should Stackbench bundle adapter CLIs or require user-managed installs
- how much of the resolved gstack should be visible before dispatch without turning the shell into a prompt editor

## Implementation Order
1. stabilize machine-readable CLI and auth contract
2. scaffold Electron shell with secure preload bridge
3. ship run list, logs, and launcher watch
4. ship adapter auth status and login flows
5. add approval and integration actions
6. add worker-type selection and editing
7. add persona presets and richer gstack preview later

## Current Implemented Slice
The current repo desktop shell includes:
- secure Electron main, preload, and renderer separation
- repo selection and runtime mode display
- adapter auth status and login actions
- run start, run list, selected run log timeline, and launcher watch feed
- markdown-backed worker-type selection in the dispatch flow
- worker-type creation and editing for repo-local `swb/profiles/*.md`
- approve, reject, and integrate actions for the selected run
- production packaging verified through `pnpm --dir desktop package`

Still pending for the next desktop milestone:
- bundling the Rust `swb` binary into packaged builds
- explicit handling for login flows that require an external terminal TTY
- Linux `.deb` verification on an actual Debian or Ubuntu host
- persona presets and richer gstack preview
- ingress visibility for Slack and Linear external refs and outbound updates

## Immediate Next Work
Future agents should prioritize this order:
1. validate `pnpm --dir desktop make` on a Debian or Ubuntu host and record the packaging result
2. bundle the Rust `swb` binary into packaged desktop builds and stop depending on Cargo in production
3. make adapter login remediation explicit when a login flow requires an external TTY
4. add filtered log views and pagination before expanding into richer workspace browsing
5. add external ref and outbound update views before Slack approval actions or Linear sync work
6. add persona presets and richer `gstack` preview only after the packaged local operator loop is solid

## Relation To Current Repo
This repo now contains a first desktop operator shell, machine-readable CLI contracts for the implemented flows, and Electron packaging that has been verified on the current host.

This document exists so future agents can extend that shell without drifting into a second orchestrator, a second state model, or a renderer that depends on scraping human terminal output.
