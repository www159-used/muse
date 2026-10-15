# Agent collaboration through pull requests

Use OpenSpec for requirements and task definitions. Use a pull request to track the implementation, review findings, and validation of a specific set of tasks.

## Work ownership

- Give each task one writing agent, one branch, and one worktree. Reviewers inspect that branch without editing the writer's directory.
- Start from the named base commit or dependency branch. Record the base and OpenSpec task IDs in the PR description.
- Keep planning changes, implementation phases, and unrelated fixes in separate PRs. If two PRs touch the same file, merge or cherry-pick commits and resolve the overlapping changes; do not copy an entire file from another worktree.
- Do not implement from the shared dirty `master` directory. Uncommitted changes there are working material, not an accepted baseline.

For a new independent task:

```sh
git fetch origin
git worktree add -b feat/task-name ../muse-task-name origin/master
```

## Progress and acceptance

Open the PR as a draft. Its description identifies scope, the writing agent, dependencies, remaining tasks, and checks run against the current head commit. Keep those fields current as scope changes.

OpenSpec checkboxes mean implementation work is complete; they do not mean the code is accepted or merged. Track acceptance separately in the PR:

- Draft: implementation or known fixes remain.
- Awaiting acceptance: the author has recorded checks for the submitted commit.
- Ready: required checks and review findings are resolved for the current head.
- Merged: the accepted commit is in the base branch.

Review findings identify a commit and file location. A new push invalidates acceptance for changed behavior until the relevant checks are rerun. Keep required CI checks as the merge gate; a test result from a changing local directory is not evidence for a different PR commit.

Keep follow-up work in the tracking PR or a separate linked PR. Do not fold another phase into a small fix merely because both touch the same UI file.
