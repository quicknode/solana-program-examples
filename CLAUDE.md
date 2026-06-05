# Project memory

Persistent instructions for AI agents working in this repository. This file is
loaded automatically at the start of each session.

## Always watch CI after opening a PR

After creating **any** pull request, immediately subscribe to its CI/activity
and follow it through to a result — do not consider the work done at "PR
opened":

- Call `subscribe_pr_activity` for the new PR so CI failures and review
  comments are delivered into the session.
- CI **failures** arrive as webhook events; CI **success** and your own pushes
  do **not**, so proactively re-check the PR's check runs
  (`pull_request_read` → `get_check_runs`) until they are green.
- On failure, pull the failing job logs (`get_job_logs`), diagnose, fix, push,
  and re-check — repeat until green or genuinely blocked (then report).

The goal: never hand back a PR that is silently red.
