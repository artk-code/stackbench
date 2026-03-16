# Stackbench v2 - AGENTS

Date: 2026-03-16
Status: Active
Depends on: `STACKBENCH_V2_ARCHITECTURE.md`, `STACKBENCH_V2_REPO_LAYOUT.md`, `STACKBENCH_V2_ROADMAP.md`, `STACKBENCH_V2_GAP_CLOSURE.md`, `STACKBENCH_V2_DESKTOP_PLAN.md`

## Purpose
Define the product intent, operating model, and ownership boundaries for Stackbench v2.

## Supporting Specs
- `STACKBENCH_V2_GAP_CLOSURE.md`
- `STACKBENCH_V2_CANONICAL_STATE.md`
- `STACKBENCH_V2_GSTACK_SPEC.md`
- `STACKBENCH_V2_ADAPTER_CONTRACT.md`
- `STACKBENCH_V2_PERSONA_PROFILE_MAPPING.md`
- `STACKBENCH_V2_EVAL_LEASE_RUNTIME.md`
- `STACKBENCH_V2_DESKTOP_PLAN.md`

## Mission
Stackbench v2 is a local-first software execution system that launches coding adapters against a repository, records canonical execution state, evaluates produced changes, and gates integration through explicit human approval.

Stackbench v2 focuses on orchestration, state ownership, and integration discipline. It does not try to be a new model framework.

## Core Principles
- Stackbench owns canonical state for tasks, runs, evaluation, approval, and integration.
- The `swb` CLI is the primary operator surface in the current slice.
- The `launcher` is the only writer into the canonical ingest path.
- An `adapter` never writes canonical state directly.
- A durable `ingest queue` exists before external integrations.
- `jj` is the primary workspace and integration model in the current slice.
- Workflows are configurable definitions, not hardcoded `manager -> coder -> reviewer` processes.

## Non-Goals
- Browser-first orchestration.
- tmux-centric execution as the core control plane.
- Direct adapter posting into canonical state.
- Slack, Linear, Redis, or other external systems as the source of truth.
- Preserving server-era monolith patterns as the target architecture.

## Canonical Ownership
TRACE owns the following records:
- task intent and metadata
- run lifecycle state
- adapter execution envelopes
- evaluation results
- approval decisions
- integration outcomes

External systems may later submit requests or receive status updates, but they do not own execution truth.

## Operator Model
The current slice assumes a single machine and a human operator using the `swb` CLI or desktop shell.

Primary operator actions:
- `swb run start`
- `swb run status`
- `swb run list`
- `swb run logs`
- `swb run approve`
- `swb run reject`
- `swb run integrate`
- `swb launcher run-once`
- `swb launcher watch`
- `swb adapter list`
- `swb adapter doctor`

`swb run start` succeeds after the launcher durably enqueues work. It does not wait for execution to begin.

Current implemented slice in the repo:
- durable enqueue through the SQLite-backed `ingest queue`
- canonical run projection in `swb-state`
- foreground launcher execution through `swb launcher run-once`
- foreground polling through `swb launcher watch`
- canonical timeline inspection through `swb run logs`
- approval, rejection, and `jj` integration commands

## Responsibility Boundaries
### Launcher
- validates run requests
- creates or references the target `workflow`
- writes requests into the durable `ingest queue`
- launches adapter processes
- receives normalized adapter execution events
- writes canonical ingest records
- advances run state through the `receiver` and `projector`

### Adapter
- prepares provider-specific execution
- runs the external coding tool
- emits normalized execution events and produced artifacts to the launcher
- reports capabilities such as auth support, streaming support, cancellation support, and artifact support

### Receiver
- accepts launcher-owned writes from the canonical ingest path
- persists raw accepted envelopes for replay and audit
- rejects malformed envelopes before projection

### Projector
- derives canonical state from accepted ingest records
- maintains stable task, run, evaluation, approval, and integration views

### Evaluator
- runs repository-defined checks inside the run workspace
- records pass or fail outcomes
- emits evaluation completion detail into the canonical run timeline
- never self-approves integration

## Current Slice Boundaries
The current slice includes:
- CLI-first orchestration
- desktop workbench over machine-readable CLI contracts
- launcher-owned canonical writes
- SQLite-backed durable ingest queue
- configurable workflows
- multi-adapter support through one normalized adapter contract
- `jj` workspaces for run isolation and integration artifacts
- repository tests as the default automated gate
- human approval before integration

The current slice excludes:
- browser-first control surfaces
- autonomous reviewer decisions
- external brokers as a requirement

- branch or PR automation as the primary artifact path
- Slack or Linear ingress

Desktop GUI work in this repo remains an operator shell over the same launcher, ingest queue, receiver, projector, and canonical state described in the supporting specs.

## Workflow Model
A `workflow` is a configured execution graph describing:
- which steps run
- which adapters are allowed
- which evaluation commands apply
- which approval rules gate integration

`manager`, `coder`, and `reviewer` may exist as named workflow steps later, but Stackbench v2 does not hardcode those roles into the architecture.

## Selective Reuse In This Repo
Useful concepts retained in the current repo:
- normalized event and state contracts in `swb-core`
- durable enqueue and replay discipline in `swb-queue-sqlite`
- auth doctor and login flows in `swb-adapters`
- canonical projection and timeline views in `swb-state`
- `jj` workflow concepts from [scripts/swb-jj.sh](scripts/swb-jj.sh)

Stackbench v2 does not treat any monolithic server-style runtime as the target design.
