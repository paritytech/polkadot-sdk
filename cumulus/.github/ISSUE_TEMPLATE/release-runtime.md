---
name: Release issue template
about: Tracking issue for new releases
title: Cumulus {{ env.VERSION }} Release checklist
---

# Release Checklist - Runtimes

## Runtimes Release

### Codebase
These checks should be performed on the codebase.

- [ ] the [`spec_version`](https://github.com/paritytech/cumulus/blob/master/docs/release.md#spec-version) has been incremented since the
    last release for any native runtimes from any existing use on public (non-private/test) networks
- [ ] previously [completed migrations](https://github.com/paritytech/cumulus/blob/master/docs/release.md#old-migrations-removed) are
    removed for any public (non-private/test) networks
- [ ] No migrations added in the last release that would need to be removed
- [ ] pallet and [extrinsic ordering](https://github.com/paritytech/cumulus/blob/master/docs/release.md#extrinsic-ordering--storage) as well as `SignedExtension`s have stayed
    the same. Bump `transaction_version` otherwise
- [ ] the [benchmarks](https://github.com/paritytech/ci_cd/wiki/Benchmarks:-cumulus) ran
- [ ] the weights have been updated for any modified runtime logic
- [ ] the various pieces of XCM config are sane

### On the release branch

The following checks can be performed after we have forked off to the release-
candidate branch or started an additional release candidate branch (rc-2, rc-3, etc)

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

---

Read more about the [release documentation](https://github.com/paritytech/cumulus/blob/master/docs/release.md).
