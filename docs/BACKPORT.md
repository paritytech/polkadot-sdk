# Backporting

This document explains how to backport a merged PR from `master` to one of the `stable*` branches.
Backports should only be used to fix bugs or security issues - never to introduce new features.

## Steps

1. Fix a bug through a PR that targets `master`.
2. Add label related to the branch to wich to backport changes to the PR.
    - `A4-backport-stable2407`
    - `A4-backport-stable2409`
    - `A4-backport-stable2412`
    - `A4-backport-stable2503`
3. Merge the PR into `master`.
4. Wait for the bot to open the backport PR.
5. Ensure the change is audited or does not need audit.
6. Merge the backport PR.(ℹ️ for the branches starting from 2412 it can be done automatically
    if backport PR has at least two reviews and a pipeline is green)

The label can also be added after the PR is merged.

ℹ️ If, for some reasons, a backport PR was not created automatically, it can be done manually.
But it is important, that the PR title follows the following pattern:
`[stableBranchName] Backport #originalPRNumber` (i.e. **[stable2412] Backport #8198**)

## Example

For example here where the dev triggered the process by adding the label after merging:

![backport](./images/backport-ex2.png)
