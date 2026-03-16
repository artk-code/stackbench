# Stackbench - gstack Specification

Date: 2026-03-16
Status: Active
Depends on: `STACKBENCH_CANONICAL_STATE.md`, `STACKBENCH_GAP_CLOSURE.md`

## Purpose
Define `gstack` as the first-class prompt composition model for Stackbench.

## Definition
A `gstack` is an ordered set of prompt layers that resolves into the runtime context given to one adapter execution.

It exists so Stackbench can:
- compose runtime rules, roles, personas, tools, and workspace context consistently
- fingerprint prompt context for auditability
- reuse prompt bundles across ingress surfaces
- separate operator-friendly names from machine-executable prompt assemblies

## Core Objects
### `gstack_id`
- stable identifier for a named prompt stack
- referenced by profiles and recorded on runs

### Layer
- one ordered prompt input
- has `layer_id`, `kind`, `source`, and optional `parameters`

### Resolved gstack
- the ordered, compatibility-checked, optionally optimized prompt material used for one run
- receives a stable `gstack_fingerprint`

## Supported Layer Kinds
- `runtime`
- `role`
- `persona`
- `tooling`
- `workspace`
- `policy`
- `task`
- `dynamic_note`

## Resolution Order
Stackbench resolves a gstack in this order:
1. runtime layer
2. role layers
3. persona layers
4. tooling layer
5. workspace layer
6. policy layer
7. task layer
8. dynamic notes added by the launcher

Later layers may refine earlier instructions but may not silently replace core runtime rules.

## Merge Rules
- order is explicit and preserved
- duplicate layers are removed only when `source` and `content_hash` match exactly
- conflicting runtime layers are invalid
- multiple role layers are allowed
- persona layers may add defaults but may not override adapter safety policy
- task layer is always terminal user-facing work intent

## Fingerprint Rule
The resolved gstack must produce:
- `gstack_id`
- `gstack_fingerprint`
- `runtime`
- ordered `layer_ids`

The fingerprint should be derived from normalized layer content plus runtime metadata so runs can be compared and audited later.

## Optimization Rules
Optimization is allowed only if it preserves behavior closely enough for audit and replay:
- remove comments
- collapse redundant whitespace
- strip cosmetic headings
- deduplicate identical boilerplate

Optimization must not:
- reorder instructions
- drop imperative statements
- change tool or policy semantics

If optimization is used, Stackbench records:
- optimization mode
- pre-optimization hash
- post-optimization hash

## Suggested File Layout
Repository-owned runtime assets:

```text
swb/
  gstacks/
  profiles/
  personas/
  prompts/
    runtime/
    roles/
    personas/
    policies/
```

This does not replace the Rust workspace. It is the repo-local asset layout used by config and runtime resolution.

## Relationship To Profiles
- a profile selects runtime and tools
- a profile references a `gstack_id` or inline stack layers
- the resolved gstack is recorded on the run

## Relationship To Personas
- a persona is an operator-facing alias or ingress preset
- personas may contribute persona-specific prompt layers into the gstack
- personas do not replace the profile

## Minimal Resolved Envelope
```json
{
  "gstack_id": "eng_review_v1",
  "gstack_fingerprint": "sha256:...",
  "runtime": "codex",
  "layers": [
    {"kind": "runtime", "source": "swb/prompts/runtime/default.md"},
    {"kind": "role", "source": "swb/prompts/roles/eng_review.md"},
    {"kind": "workspace", "source": "generated"},
    {"kind": "task", "source": "operator_input"}
  ]
}
```

## Relation To Current Repo Baseline
The current repo has config and runtime boundaries where a first-class prompt stack can land cleanly, but it does not yet have:
- first-class prompt stack identity
- reusable layer resolution
- prompt fingerprints
- a distinction between role prompt, persona alias, and runtime profile

`gstack` is the worthwhile addition from the external bundle and should become the reusable prompt abstraction going forward.
