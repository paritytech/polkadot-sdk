---
name: fix-prdoc-semver
description: This skill should be used when the user asks to "check prdoc semver", "verify prdoc bumps", "fix prdoc version bumps", "analyze PR semver changes", "review crate bumps in prdoc", or wants to ensure the semver bump levels in a prdoc file match the actual code changes in a PR.
version: 0.1.0
---

# Fix prdoc Semver Bumps

Verify and correct the `bump` field for every crate listed in a `prdoc/pr_<NUMBER>.prdoc` file by analyzing the actual code changes in the corresponding GitHub PR.

## When to Use

Invoke this skill when a prdoc file exists (or needs to be validated) and the semver bump levels need to be checked against the real diff. This is common before merging a PR or when a reviewer flags incorrect version bumps.

## Inputs

The skill needs a **PR number**. Determine it from one of:
1. An explicit argument (e.g. `/fix-prdoc-semver 6029`).
2. The prdoc file the user references.
3. The current branch name if it encodes a PR number.

## Prerequisites

1. Make sure the `gh` command (GitHub CLI) is available and the user is logged in (`gh auth status`). If `gh` is not installed or not authenticated, stop and ask the user to install it and authenticate.
2. Make sure the current directory is a Git worktree of `paritytech/polkadot-sdk` GitHub repository. If not, stop and ask user to run this skill in a proper worktree.
3. Make sure the PR number (refer to "Inputs" section) is known. If not, stop and ask user to provide the PR number.
4. Make sure `prdoc/pr_<NUMBER>.prdoc` exists, is not empty, and includes at least one crate in the `crates` array. If not, stop and inform the user.

## Procedure

### 1. Read the prdoc file

Read `prdoc/pr_<NUMBER>.prdoc` from the working tree. Extract the list of crates and their current `bump` values.

### 2. Obtain the PR diff

Fetch the full diff with:

```
gh pr diff <NUMBER> --repo paritytech/polkadot-sdk
```

From the diff, build a mapping of **changed files grouped by crate**. Use `Cargo.toml` locations and directory conventions to associate files with crate names:
- `substrate/client/*` -> `sc-*` crates
- `substrate/frame/*` -> `frame-*` or `pallet-*` crates
- `substrate/primitives/*` -> `sp-*` crates
- `cumulus/client/*` -> `cumulus-client-*` crates
- `cumulus/pallets/*` -> `cumulus-pallet-*` crates
- `polkadot/xcm/*` -> various xcm crates

### 3. Analyze each crate

For every crate listed in the prdoc, inspect the diff hunks that belong to it and classify the change:

#### MAJOR — incompatible API changes

Assign `major` when the crate's **public API** has a breaking change. Common patterns:

- A public trait method gains, removes, or reorders parameters.
- A public struct/enum field is added (if non-exhaustive is not used), removed, or changes type.
- A public function signature changes (parameters, return type, generics, trait bounds).
- A public type alias changes its target.
- A proc-macro's generated code changes in a way that requires callers to update.

#### MINOR — new backward-compatible functionality

Assign `minor` when **new public API surface** is added without breaking existing API:

- A new public function, method, constant, or type is added.
- A new storage item is added to a FRAME pallet.
- A new variant is added to a non-exhaustive enum.
- Behavior of an existing public function changes but is gated behind a feature flag or version check (e.g. `system_version >= 3`) and is backward-compatible for existing users.
- A new dependency is added to `Cargo.toml` that appears in the public API.

#### PATCH — backward-compatible internal changes

Assign `patch` when there is **no public API change**:

- Internal implementation changes (e.g. an internal function now passes an extra argument to a dependency).
- Call-site adaptations to upstream API changes with no effect on this crate's own public API.
- Doc comment changes, typo fixes, variable renames in non-public code.
- Test-only or benchmark-only changes.
- A crate is listed in the prdoc but has **no changed files at all** in the diff (transitive dependency bump).

### 4. Special considerations for this repository

- **FRAME pallets**: Adding a new storage item or dispatchable is MINOR. Changing the behavior of an existing dispatchable in a backward-compatible way (gated by runtime version) is MINOR. Changing a `Config` trait (adding required associated types) is MAJOR.
- **Trait implementations for existing types**: If a crate only updates its internal trait impl to match an upstream trait signature change (e.g. passing a new parameter), but the crate's own public API is unchanged, that is PATCH.
- **Proc macros (`*-proc-macro`)**: The "API" of a proc macro is its generated output. If the generated code changes in a way that only compiles against a new version of a dependency trait, that is MAJOR.

### 5. Update the prdoc

Edit `prdoc/pr_<NUMBER>.prdoc`, changing each crate's `bump` value to match the analysis. Preserve the file's existing structure, ordering, and formatting.

### 6. Report

Present a summary table to the user:

```
| Crate | Was | Now | Reason |
|-------|-----|-----|--------|
| ...   | ... | ... | ...    |
```

Always include all the crates in the table, even if the bump did not change.

## Semver Quick Reference (from semver.org)

- **MAJOR**: incompatible API changes
- **MINOR**: add functionality in a backward compatible manner
- **PATCH**: backward compatible bug fixes / internal changes

The key question for each crate is: **does this crate's own public API change?**
- If a public signature changed or was removed -> MAJOR
- If new public API was added without breaking existing API -> MINOR
- If only internals changed -> PATCH
