# Stackbench - Evaluation And Lease Runtime

Date: 2026-03-16
Status: Active
Depends on: `STACKBENCH_CANONICAL_STATE.md`, `STACKBENCH_GAP_CLOSURE.md`

## Purpose
Define the deterministic evaluation contract and the lease fencing model used when Stackbench executes task-scoped work across competing workers or ingress surfaces.

## Evaluation Runtime
### Inputs
- `run_id`
- `task_id`
- `step_id?`
- workspace root
- evaluator configuration
- artifact references from adapter execution

### Outputs
- `evaluation_id`
- `passed`
- `score?`
- result rows
- artifact refs to reports or fixtures

### Minimum Result Row
- `command`
- `success`
- `exit_code`
- `stdout?`
- `stderr?`

### Determinism Rules
- evaluation commands must be repository-defined
- inputs must be explicit and replayable
- pass/fail semantics must not depend on UI interpretation
- when scoring is used, thresholds must be part of config

### Canonical Evaluation States
- `queued`
- `running`
- `passed`
- `failed`
- `error`

### Integration Gate
- evaluation must complete before `awaiting_review`
- failed evaluation moves the run to `failed`
- passed evaluation moves the run to `awaiting_review`

## Lease Runtime
Leases are required for any task-scoped execution where more than one worker or ingress path could race on the same task.

### Canonical Lease Fields
- `task_id`
- `holder`
- `lease_epoch`
- `active`
- `expires_at?`

### Lease Operations
- `claim`
- `renew`
- `release`
- `expire`

### Claim Rules
- claim increments epoch
- only one active holder exists per task
- claim requests may include expected epoch for stale-write protection

### Renew Rules
- renew requires matching holder
- renew requires matching epoch

### Release Rules
- release requires matching holder
- release requires matching epoch

### Write Fencing Rule
Any task-scoped write emitted by a worker must carry:
- `task_id`
- `run_id`
- `lease_epoch`
- `worker_id`

If the carried epoch is stale relative to the canonical lease state, Stackbench must reject or mark the write stale.

## Phase Rule
### Phase 0 Local Core
- launcher-owned, local queue execution may run without explicit remote lease heartbeats
- serialized local execution reduces contention inside one machine

### Phase 1 And Beyond
- Slack ingress, remote launchers, or competing workers must use explicit lease fencing
- stale writes must be rejected deterministically

## Relation To Current Repo Baseline
The current repo already has the local evaluation gate and the canonical run-state progression needed for the workbench loop.

Lease fencing remains a spec requirement for later competing workers or ingress paths. The worthwhile move for Stackbench is to keep lease semantics explicit and ready for that future transport layer, while keeping the current local evaluation contract deterministic rather than aggregation-only.
