---
name: Release issue template
about: Tracking issue for new releases
title: Cumulus {{ env.VERSION }} Release checklist
---

# Release Checklist - Runtimes

## Runtimes Release

### Codebase
These checks should be performed on the codebase.

- [ ] the [`spec_version`](../../docs/release.md#spec-version) has been incremented since the
    last release for any native runtimes from any existing use on public (non-private/test) networks
- [ ] previously [completed migrations](../../docs/release.md#old-migrations-removed) are
    removed for any public (non-private/test) networks
- [ ] No migrations added in the last release that would need to be removed
- [ ] pallet and [extrinsic ordering](../../docs/release.md#extrinsic-ordering) as well as `SignedExtension`s have stayed
    the same. Bump `transaction_version` otherwise
- [ ] the [benchmarks](../../docs/release.md#benchmarks) ran
- [ ] the weights have been updated for any modified runtime logic
- [ ] the various pieces of XCM config are sane

### On the release branch

The following checks can be performed after we have forked off to the release-
candidate branch or started an additional release candidate branch (rc-2, rc-3, etc)

- [ ] Verify [new migrations](../../docs/release.md#new-migrations) complete successfully, and the
    runtime state is correctly updated for any public (non-private/test)
    networks
- [ ] Run integration tests
- [ ] Push runtime upgrade to Westmint and verify network stability


### Github

- [ ] Check that a draft release has been created at the [Github Releases page](https://github.com/paritytech/cumulus/releases) with relevant [release
    notes](../../docs/release.md#release-notes)
- [ ] Check that [build artifacts](../../docs/release.md#build-artifacts) have been added to the
    draft-release.

---

Read more about the [release documentation](../../docs/release.md).
