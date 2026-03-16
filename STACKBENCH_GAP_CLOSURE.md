# Stackbench - Gap Closure

Date: 2026-03-16
Status: Active
Depends on: `STACKBENCH_AGENTS.md`, `STACKBENCH_ARCHITECTURE.md`, `STACKBENCH_REPO_LAYOUT.md`, `STACKBENCH_ROADMAP.md`

## Purpose
Turn the external runtime bundle review into a concrete set of missing specs that Stackbench should adopt before Slack ingress or desktop orchestration becomes the primary operator path.

## Why This Exists
The external bundle introduced useful ideas:
- prompt stack composition
- persona and profile layering
- multi-runtime CLI adapters
- Slack ingress
- explicit lease and evaluation stages

The bundle did not define enough runtime contract to implement those safely. Stackbench needs a tighter spec surface first.

## Gap Closure Items
1. Canonical state and IDs
   - source of truth for tasks, runs, steps, leases, evaluations, approvals, integrations, and timeline events
   - document: `STACKBENCH_CANONICAL_STATE.md`
2. First-class `gstack`
   - ordered prompt-layer composition with stable fingerprints and compatibility rules
   - document: `STACKBENCH_GSTACK_SPEC.md`
3. Adapter I/O and auth contract
   - normalized execution events, auth status, login, cancellation, and artifact semantics
   - document: `STACKBENCH_ADAPTER_CONTRACT.md`
4. Persona and profile mapping
   - separate ingress aliases from machine-executable runtime profiles
   - document: `STACKBENCH_PERSONA_PROFILE_MAPPING.md`
   - current executable slice: `swb persona *`, `swb run start --persona`, and repo-local persona files under `swb/personas/`
5. Evaluation and lease runtime
   - deterministic evaluation contract and network-safe lease fencing model
   - document: `STACKBENCH_EVAL_LEASE_RUNTIME.md`
6. External ingress
   - Slack and Linear request surfaces over the same queue path
   - document: `STACKBENCH_INGRESS_SPEC.md`

## Comparison To Current Repo
What already exists in the current repo:
- canonical state, run IDs, and replay-safe timeline persistence
- adapter auth status and login flow
- local evaluation, approval, and `jj` integration
- desktop shell over machine-readable CLI contracts
- markdown-backed worker types with minimal gstack resolution and fingerprinting
- persona resolution and initial Slack/Linear ingress
- the retained `jj` helper flow

What is still intentionally incomplete:
- richer multi-layer `gstack` resolution and preview
- persona presets in the desktop shell
- broader multi-adapter auth parity
- outbound Slack and Linear delivery
- Slack approval actions and Linear sync
- lease fencing across multiple ingress surfaces
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

The resulting desktop follow-on plan lives in `STACKBENCH_DESKTOP_PLAN.md`, which depends on these gap closures rather than replacing them.
