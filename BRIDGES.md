# Using Parity Bridges Common dependency (`git subtree`)

In `./bridges` sub-directory you can find a `git subtree` imported version of:
[`parity-bridges-common`](https://github.com/paritytech/parity-bridges-common/) repository.

(For regular Cumulus contributor 1. is relevant) \
(For Cumulus maintainer 1. and 2. are relevant) \
(For Bridges team 1. and 2. and 3. are relevant)

## How to fix broken Bridges code?

To fix Bridges code simply create a commit in current (`Cumulus`) repo. Best if
the commit is isolated to changes in `./bridges` sub-directory, because it makes
it easier to import that change back to upstream repo.

(Any changes to `bridges` subtree require Bridges team approve and they should manage backport to Bridges repo)


## How to pull latest Bridges code to the `bridges` subtree
(in practice)

The `bridges` repo has a stabilized branch `polkadot-staging` dedicated for releasing.

```
cd <cumulus-git-repo-dir>

# this will update new git branches from bridges repo
# there could be unresolved conflicts, but don't worry,
# lots of them are caused because of removed unneeded files with patch step,
BRANCH=polkadot-staging ./scripts/bridges_update_subtree.sh fetch

# so, after fetch and before solving conflicts just run patch,
# this will remove unneeded files and checks if subtree modules compiles
./scripts/bridges_update_subtree.sh patch

# if there are conflicts, this could help,
# this removes locally deleted files at least (move changes to git stash for commit)
./scripts/bridges_update_subtree.sh merge

# (optional) when conflicts resolved, you can check build again - should pass
# also important: this updates global Cargo.lock
./scripts/bridges_update_subtree.sh patch

# add changes to the commit, first command `fetch` starts merge,
# so after all conflicts are solved and patch passes and compiles,
# then we need to finish merge with:
git merge --continue
```

## How to pull latest Bridges code or contribute back?
(in theory)

Note that it's totally fine to ping the **Bridges Team** to do that for you. The point
of adding the code as `git subtree` is to **reduce maintenance cost** for Cumulus/Polkadot
developers.

If you still would like to either update the code to match latest code from the repo
or create an upstream PR read below. The following commands should be run in the
current (`polkadot`) repo.

### Add Bridges repo as a local remote
```
git remote add -f bridges git@github.com:paritytech/parity-bridges-common.git
```

If you plan to contribute back, consider forking the repository on Github and adding
your personal fork as a remote as well.
```
git remote add -f my-bridges git@github.com:tomusdrw/parity-bridges-common.git
```

### To update Bridges
```
git fetch bridges polkadot-staging
git subtree pull --prefix=bridges bridges polkadot-staging --squash
```

We use `--squash` to avoid adding individual commits and rather squashing them
all into one.

### Clean unneeded files here
```
./bridges/scripts/verify-pallets-build.sh --ignore-git-state --no-revert
```

### Contributing back to Bridges (creating upstream PR)
```
git subtree push --prefix=bridges my-bridges polkadot-staging
```
This command will push changes to your personal fork of Bridges repo, from where
you can simply create a PR to the main repo.
