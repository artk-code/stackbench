# Stackbench - Persona And Profile Mapping

Date: 2026-03-16
Status: Active
Depends on: `STACKBENCH_GSTACK_SPEC.md`, `STACKBENCH_ADAPTER_CONTRACT.md`, `STACKBENCH_GAP_CLOSURE.md`

## Purpose
Separate human-facing invocation names from machine-executable runtime profiles.

## Terms
### Persona
- ingress-facing alias used by a human or external system
- examples: `eng-review`, `qa`, `review`, `ship`
- may carry defaults for profile, adapter, tools, or approval policy
- in the current repo, personas are repo-local TOML files under `swb/personas/<ingress>/`

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
    linear/
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
- current repo supports `POST /ingress/slack/command` and `POST /ingress/slack/action`
- current repo includes `slack-review` and `slack-deploy` personas under `swb/personas/slack/`

### Desktop
- operator selects profile directly or through persona presets
- the current desktop shell already surfaces profile selection and editing
- desktop may hide persona if it is not useful outside ingress contexts
- future desktop work can surface ingress refs and outbound updates using the same canonical IDs

### CLI
- operator may specify `--profile`
- optional `--persona` is implemented when ingress parity matters
- `swb persona list|show|save` manages repo-local personas
- `swb run start --persona <PERSONA_ID>` resolves the same mapping path ingress uses

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

The current repo now exposes both sides of this split:
- profiles are markdown-backed worker types under `swb/profiles`
- personas are TOML ingress aliases under `swb/personas/`
- CLI dispatch supports both `--profile` and `--persona`
- Slack and Linear ingress resolve personas into the same profile and gstack model

What remains is richer presentation:
- persona presets in the desktop shell
- clearer persona/profile/gstack preview
- cross-ingress lease fencing once multiple remote request surfaces are active
