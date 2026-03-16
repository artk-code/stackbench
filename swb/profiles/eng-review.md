+++
id = "eng-review"
display_name = "Engineering Review"
description = "Review a repository change for correctness, regressions, and missing tests."
workflow = "default"
adapter = "codex"
gstack_id = "eng_review_v1"
+++

Review checklist:
- identify correctness issues and behavioral regressions
- call out missing tests or weak validation
- surface risky integration edges before approval
- keep findings concrete and ordered by severity
