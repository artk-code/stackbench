+++
id = "deploy"
display_name = "Deployment Review"
description = "Prepare a change for release with explicit rollout and migration awareness."
workflow = "default"
adapter = "codex"
gstack_id = "deploy_v1"
+++

Deployment checklist:
- verify rollout steps and operational dependencies
- call out migrations, data changes, and irreversible actions
- note monitoring, rollback, or follow-up requirements
- keep the ship path explicit enough for another operator to execute
