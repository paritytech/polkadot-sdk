---
name: Release Checklist for Runtime
about: Release Checklist for Runtime
title: Release Checklist for Runtime {{ env.VERSION }}
---

# Release Checklist - Runtimes

**All** following checks must be completed before publishing a new release.
The release process is owned and led by @paritytech/release-engineering team.
The checks marked with :crab: are meant to be checked by [a runtime engineer](https://github.com/paritytech/cumulus/issues/1761).

## Runtimes Release

### Codebase
These checks should be performed on the codebase.

- [ ] the [`spec_version`](https://github.com/paritytech/cumulus/blob/master/docs/release.md#spec-version) has been incremented since the
    last release for any native runtimes from any existing use on public (non-private/test) networks
- [ ] :crab: previously [completed migrations](https://github.com/paritytech/cumulus/blob/master/docs/release.md#old-migrations-removed) are removed for any public (non-private/test) networks
- [ ] pallet and [extrinsic ordering](https://github.com/paritytech/cumulus/blob/master/docs/release.md#extrinsic-ordering--storage) as well as `SignedExtension`s have stayed
    the same. Bump `transaction_version` otherwise
- [ ] the [benchmarks](https://github.com/paritytech/ci_cd/wiki/Benchmarks:-cumulus) ran
- [ ] the weights have been updated for any modified runtime logic
- [ ] :crab: the new weights are sane, there are no significant (>50%) drops or rises with no reason
- [ ] :crab: XCM config is compatible with the configurations and versions of relevant interlocutors, like the Relay Chain.

### On the release branch

The following checks can be performed after we have forked off to the release-candidate branch or started an additional release candidate branch (rc-2, rc-3, etc)

- [ ] Verify [new migrations](https://github.com/paritytech/cumulus/blob/master/docs/release.md#new-migrations) complete successfully, and the
    runtime state is correctly updated for any public (non-private/test)
    networks
- [ ] Run [integration tests](https://github.com/paritytech/cumulus/blob/master/docs/release.md#integration-tests), and make sure they pass.
- [ ] Push runtime upgrade to Westmint and verify network stability


### Github

- [ ] Check that a draft release has been created at the [Github Releases page](https://github.com/paritytech/cumulus/releases) with relevant [release
    notes](https://github.com/paritytech/cumulus/blob/master/docs/release.md#release-notes)
- [ ] Check that [build artifacts](https://github.com/paritytech/cumulus/blob/master/docs/release.md#build-artifacts) have been added to the
    draft-release.

# Post release

- [ ] :crab: all commits (runtime version bumps, fixes) on this release branch have been merged back to master.

---

Read more about the [release documentation](https://github.com/paritytech/cumulus/blob/master/docs/release.md).
