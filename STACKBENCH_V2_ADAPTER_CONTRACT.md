# Stackbench v2 - Adapter Contract

Date: 2026-03-16
Status: Active
Depends on: `STACKBENCH_V2_CANONICAL_STATE.md`, `STACKBENCH_V2_GSTACK_SPEC.md`, `STACKBENCH_V2_GAP_CLOSURE.md`

## Purpose
Define the normalized I/O, auth, and lifecycle contract for all Stackbench v2 adapters.

## Adapter Goal
An adapter wraps one runtime-specific tool and makes it look like the same execution surface to the launcher.

Examples:
- Codex CLI
- Claude Code
- Gemini CLI

## Required Operations
- `doctor`
- `auth_status`
- `login`
- `prepare`
- `launch`
- `stream_events`
- `cancel`
- `collect_artifacts`
- `cleanup`

Phase 0 may implement these as launcher-owned subprocess calls rather than a trait object API, but the normalized behavior must stay the same.

## Adapter Input
Minimum launch context:
- `run_id`
- `task_id`
- `step_id`
- `profile_id?`
- `persona_id?`
- `gstack_fingerprint`
- `workspace_root`
- resolved prompt input
- adapter-specific runtime configuration

## Normalized Event Envelope
Every adapter event emitted to Stackbench must contain:
- `run_id`
- `step_id`
- `adapter`
- `ts`
- `event_kind`
- `payload`

Minimum event kinds:
- `prepared`
- `launched`
- `stdout`
- `stderr`
- `warning`
- `error`
- `command_completed`
- `cancelled`
- `artifact_discovered`

## Completion Payload
At minimum:
- `success`
- `exit_code`
- `stdout?`
- `stderr?`
- `duration_ms`
- `artifact_refs?`
- `change_id?`
- `bookmark?`

## Artifact Contract
Adapters may produce:
- changed workspace files
- diff or patch references
- revision identifiers
- bookmarks
- structured summaries

Artifacts must be recorded by reference where possible. Canonical state stores metadata first, not arbitrary large blobs by default.

## Auth Contract
Every adapter declares:
- `auth_strategy`
- `login_supported`
- `device_login_supported`

Supported auth strategies:
- `none`
- `codex_login_status`
- `command_status`

Normalized auth status response:
- `name`
- `command`
- `available`
- `logged_in?`
- `auth_method?`
- `login_supported`
- `device_login_supported`
- `login_command?`
- `device_login_command?`
- `detail`

Normalized login result:
- `name`
- `mode`
- `available`
- `success`
- `exit_code`
- `command?`
- `stdout`
- `stderr`
- `detail`

## CLI Surface
Stackbench should expose adapter contract operations through machine-readable CLI paths:
- `swb adapter list --json`
- `swb adapter doctor --json`
- `swb adapter auth status [ADAPTER] --json`
- `swb adapter auth login <ADAPTER> [--device] --json`

These are stable integration points for a desktop shell or future remote controller.

## Safety Rules
- adapters never write canonical state directly
- adapters never decide approval or integration
- adapters do not bypass workspace isolation
- auth checks in the GUI must go through the same launcher or CLI contract as terminal use

## Unsupported Behavior
Stackbench v2 should reject adapter integrations that require:
- scraping unstructured terminal output with no stable success semantics
- direct mutation of canonical state
- hidden network callbacks into the state receiver
- runtime-specific prompt assembly outside the gstack contract

## Relation To Current Repo Baseline
The current repo already has:
- generic adapter registry
- normalized execution summary events
- adapter auth status and login CLI surface

What this doc adds is the stable contract that makes those behaviors reusable across desktop and future remote surfaces.
