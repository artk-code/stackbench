# Stackbench - Canonical State

Date: 2026-03-16
Status: Active
Depends on: `STACKBENCH_ARCHITECTURE.md`, `STACKBENCH_GAP_CLOSURE.md`

## Purpose
Define the canonical identifiers, record types, ownership rules, and lifecycle transitions used across Stackbench.

## Ownership Rule
Stackbench owns canonical state for:
- task intent
- run lifecycle
- step execution
- evaluation results
- approval decisions
- integration outcomes
- accepted timeline events

External systems may request work or receive status, but they do not become the source of execution truth.

## Canonical IDs
### `task_id`
- stable task identity across ingress surfaces
- may come from operator input or external ingress mapping
- required for any task-scoped execution, lease, evaluation, or approval flow

### `run_id`
- globally unique execution identity
- created by Stackbench when work is accepted
- never reused

### `step_id`
- workflow-local execution step name
- examples: `primary`, `evaluation`, `review`
- unique within a run

### `ingest_entry_id`
- monotonic durable identifier assigned by the ingest queue
- defines replay order inside one canonical ingest domain

### `evaluation_id`
- unique identity for one evaluation pass over one run or step result

### `approval_id`
- unique identity for one human approval or rejection decision

### `integration_id`
- unique identity for one integration attempt

### `workspace_id`
- derived as `<run_id>/<step_id>` for workspace isolation and artifact lookup

## Canonical Records
### Task
- `task_id`
- `title`
- `origin`
- `requested_by`
- `status`
- `current_run_id?`
- `current_lease_epoch?`

### Run
- `run_id`
- `task_id`
- `workflow`
- `adapter`
- `profile_id?`
- `persona_id?`
- `gstack_id?`
- `state`
- `created_at`
- `updated_at`
- `last_error?`

### Step Execution
- `run_id`
- `step_id`
- `adapter`
- `state`
- `workspace_id`
- `started_at?`
- `completed_at?`
- `summary?`

### Lease State
- `task_id`
- `holder`
- `lease_epoch`
- `active`
- `expires_at?`

### Evaluation
- `evaluation_id`
- `run_id`
- `step_id?`
- `evaluator`
- `status`
- `score?`
- `passed`
- `result_path?`

### Approval
- `approval_id`
- `run_id`
- `decision`
- `decided_by`
- `reason?`
- `decided_at`

### Integration
- `integration_id`
- `run_id`
- `status`
- `change_id?`
- `bookmark?`
- `detail?`

### Run Event
- `ingest_entry_id`
- `run_id`
- `ts`
- `kind`
- `payload`
- `applied_at`

## Lifecycle States
### Task state
- `draft`
- `queued`
- `leased`
- `running`
- `evaluating`
- `awaiting_review`
- `integrated`
- `failed`
- `cancelled`

### Run state
- `draft`
- `queued`
- `running`
- `evaluating`
- `awaiting_review`
- `approved`
- `rejected`
- `integrated`
- `archived`
- `failed`
- `cancelled`

### Step state
- `queued`
- `running`
- `completed`
- `failed`
- `cancelled`

## State Transition Rules
- Only accepted ingest records may move canonical state.
- A run may not move backward in lifecycle except by replay rebuilding the same derived result.
- Approval does not imply integration.
- Integration is valid only from `approved`.
- Evaluation is required before `awaiting_review`.
- Task-level lease state and run-level lifecycle are related but not interchangeable.

## Minimal Accepted Event Kinds
- `run_requested`
- `run_started`
- `adapter_event`
- `run_evaluating`
- `run_awaiting_review`
- `run_approved`
- `run_rejected`
- `run_integrated`
- `run_failed`
- `run_cancelled`

Phase 1 may add task- and lease-scoped event kinds, but they must preserve the same canonical IDs.

## Replay Rule
- canonical state must be reconstructable from accepted ingest entries
- derived tables may be rebuilt without losing identity semantics
- any optimization cache must remain disposable

## Relation To Current Repo Baseline
The current repo already carries the current state model in executable form:
- canonical identifiers and event kinds in `swb-core`
- durable ingest ordering in `swb-queue-sqlite`
- replay-safe projection and timeline persistence in `swb-state`
- evaluation and integration progression across `swb-launcher`, `swb-eval`, and `swb-jj`

The next repo cutover should preserve these boundaries even when internal `swb` names are eventually renamed to `stackbench`.
