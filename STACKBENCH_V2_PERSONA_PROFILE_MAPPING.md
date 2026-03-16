# Stackbench v2 - Persona And Profile Mapping

Date: 2026-03-16
Status: Active
Depends on: `STACKBENCH_V2_GSTACK_SPEC.md`, `STACKBENCH_V2_ADAPTER_CONTRACT.md`, `STACKBENCH_V2_GAP_CLOSURE.md`

## Purpose
Separate human-facing invocation names from machine-executable runtime profiles.

## Terms
### Persona
- ingress-facing alias used by a human or external system
- examples: `eng-review`, `qa`, `review`, `ship`
- may carry defaults for profile, adapter, tools, or approval policy

### Profile
- machine-readable runtime definition
- selects adapter runtime, tools, workspace requirements, and gstack

### Role Prompt
- one prompt layer usually referenced by a gstack

### gstack
- resolved prompt-layer assembly used at runtime

## Mapping Rule
- personas map to profiles
- profiles map to adapters, tools, workspace policy, and gstack
- runs record both `persona_id?` and `profile_id?`

One persona may map to one default profile.
One profile may be used by multiple personas or non-Slack entry points.

## Example
### Persona
```toml
id = "eng-review"
display_name = "Engineering Review"
default_profile = "eng_review_codex"
default_workflow = "default"
description = "Review a repository change for implementation quality and risk."
```

### Profile
```toml
id = "eng_review_codex"
runtime = "codex"
gstack_id = "eng_review_v1"
tools = ["repo_read", "repo_write", "web_search"]
approval_policy = "human_required"

[workspace]
requires_repo = true
requires_browser = false
```

## Suggested File Layout
```text
swb/
  personas/
    slack/
  profiles/
  prompts/
    roles/
  gstacks/
```

## Ingress Mapping
### Slack
- slash command chooses persona
- persona selects default profile
- Slack request payload becomes task input

### Desktop
- operator selects profile directly or through persona presets
- desktop may hide persona if it is not useful outside ingress contexts

### CLI
- operator may specify `--profile`
- optional `--persona` is allowed when ingress parity matters

## Runtime Mapping Rules
- profile chooses adapter runtime
- profile references one gstack or inline layer list
- persona may add ingress-only metadata but not mutate core adapter safety policy
- approval policy is defined by profile or workflow, not by Slack command text alone

## Worthwhile Addition From External Bundle
The external docs correctly separate:
- role prompt content
- invocation alias
- runtime behavior

The current repo has the runtime and config boundaries needed for a persona/profile split, but it does not yet expose that split through the CLI or desktop shell. Stackbench v2 should adopt the persona/profile model without collapsing it into adapter names or workflow shortcuts.
