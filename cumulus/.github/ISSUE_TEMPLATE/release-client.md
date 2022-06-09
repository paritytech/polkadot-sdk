---
name: Release Checklist for Client - issue template
about: Release Checklist for Client
title: Release Checklist - Client {{ env.VERSION }}
---

# Release Checklist - Client

### Client Release

- [ ] build a new `polkadot-parachain` binary and publish it to S3
- [ ] new `polkadot-parachain` version has [run on the network](../../docs/release.md#burnin)
    without issue for at least 12h
- [ ] a draft release has been created in the [Github Releases page](https://github.com/paritytech/cumulus/releases) with the relevant release-notes
- [ ] the [build artifacts](../../docs/release.md#build-artifacts) have been added to the
    draft-release.

---

Read more about the [release documentation](../../docs/release.md).
