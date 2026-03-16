# Stackbench v2 - Gap Closure

Date: 2026-03-16
Status: Active
Depends on: `STACKBENCH_V2_AGENTS.md`, `STACKBENCH_V2_ARCHITECTURE.md`, `STACKBENCH_V2_REPO_LAYOUT.md`, `STACKBENCH_V2_ROADMAP.md`

## Purpose
Turn the external runtime bundle review into a concrete set of missing specs that Stackbench v2 should adopt before Slack ingress or desktop orchestration becomes the primary operator path.

## Why This Exists
The external bundle introduced useful ideas:
- prompt stack composition
- persona and profile layering
- multi-runtime CLI adapters
- Slack ingress
- explicit lease and evaluation stages

The bundle did not define enough runtime contract to implement those safely. Stackbench v2 needs a tighter spec surface first.

## Gap Closure Items
1. Canonical state and IDs
   - source of truth for tasks, runs, steps, leases, evaluations, approvals, integrations, and timeline events
   - document: `STACKBENCH_V2_CANONICAL_STATE.md`
2. First-class `gstack`
   - ordered prompt-layer composition with stable fingerprints and compatibility rules
   - document: `STACKBENCH_V2_GSTACK_SPEC.md`
3. Adapter I/O and auth contract
   - normalized execution events, auth status, login, cancellation, and artifact semantics
   - document: `STACKBENCH_V2_ADAPTER_CONTRACT.md`
4. Persona and profile mapping
   - separate ingress aliases from machine-executable runtime profiles
   - document: `STACKBENCH_V2_PERSONA_PROFILE_MAPPING.md`
5. Evaluation and lease runtime
   - deterministic evaluation contract and network-safe lease fencing model
   - document: `STACKBENCH_V2_EVAL_LEASE_RUNTIME.md`

## Comparison To Current Repo
What already exists in the current repo:
- canonical state, run IDs, and replay-safe timeline persistence
- adapter auth status and login flow
- local evaluation, approval, and `jj` integration
- desktop shell over machine-readable CLI contracts
- the retained `jj` helper flow

What is still intentionally incomplete:
- first-class `gstack`
- persona and profile mapping model
- broader multi-adapter auth parity
- Slack adapter contract
- deterministic evaluation pack and scoring contract
- richer packaged desktop behavior around bundled binaries and login remediation

## Adoption Rule
Only adopt ideas from the external bundle that improve:
- canonical ownership
- prompt/runtime composition
- operator safety
- multi-adapter portability
- deterministic evaluation

Do not adopt:
- vague role naming without machine-readable mapping
- adapter contracts that rely on prompt scraping or human parsing
- lease language without explicit epoch/write semantics
- Slack-first assumptions before the local core is stable

The resulting desktop follow-on plan lives in `STACKBENCH_V2_DESKTOP_PLAN.md`, which depends on these gap closures rather than replacing them.
