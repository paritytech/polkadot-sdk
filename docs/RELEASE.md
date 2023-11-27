# Release

The output of a release are the `polkadot` node, runtimes for the Westend & Rococo networks and new versions of the crates published to `crates.io`.

## Versioning

We are releasing multiple different things from this repository in one release, but 
we don't want to use the same version for everything. Thus, in the following we explain
the versioning story for the crates, node and Westend & Rococo. To easily refer to a release, we shall use the node version of it.

### Crate

We try to follow SemVer<sup>3</sup> as best as possible for versioning our crates' public APIs.  

👉 The public API of our library crates is defined as all public items that are not `#[doc(hidden)]`.

### Node

The versioning of the node is done 99% of the time by only incrementing the `minor` version. 
The `major` version is only bumped for special releases and the `patch` can be used for an 
out of band release that fixes some critical bug. The node version is not following SemVer. 
This means that the version doesn't express if there are any breaking changes in the CLI 
interface or similar. The node version is declared in the `NODE_VERSION` variable in 
`polkadot/node/primitives/src/lib.rs`.

### Westend & Rococo

For the these networks, we only increment the `spec_version`. The spec version is also following
the node version. The schema is as follows: `M_mmm_ppp` and for example `1_002_000` is the node release `1.2.0`. This versioning has no further meaning, and is only done to map from an on chain `spec_version` easily to the release in this repository. 

## Backports

Backports should most of the time not be required. We should only backport critical bug fixes and then release the fixed crates. There should be no need to backport anything from a release branch.

When a backport is required for some previous release, it is the job of the developer (assuming it is some internal person) that has created the initial PR to create the backports. After the backports are done, it is important to ensure that the crate release is being done. We should backport fixes to the releases of the last 6 months.

# Processes

The following processes are necessary to actualize our releases. Each process has a *Cadence* on which it must execute and an *Responsible* that is responsible for autonomously doing so and reporting back any error in the RelEng<sup>1</sup> channel.

## Crate Bumping

Cadence: Each Merge Request. Responsible: Developer that opened the MR.

Following SemVer isn't easy, but there exists [a guide](https://doc.rust-lang.org/cargo/reference/semver.html) in the Rust documentation that explains the small details on when to bump what. This process should be augmented with CI checks that utilize [`cargo-semver-checks`](https://github.com/obi1kenobi/cargo-semver-checks) and/or [`cargo-public-api`](https://github.com/Enselic/cargo-public-api).

### Steps

1. [ ] Developer opens a Merge Request with changed crates.
3. [ ] They bump all changed crates according to SemVer.
4. [ ] They bump all crates that export any changed types in their *public API*.
5. [ ] They also bump all crates that inherit logic changes from relying on one of the bumped crates. 

## Mainline Release

Cadence: every two weeks. Responsible: Release Team.

This process aims to release the `release` branch as a *Mainline* release every two weeks. It should eventually be automated.

### Steps

1. [ ] Check if process [Clobbering](#clobbering) needs to happen and do so first, if that is the case.
1. [ ] Check out the latest commit of `release`.
2. [ ] Verify all CI checks of that commit.
3. [ ] Announce that commit as cutoff *Mainline* for a release in the General<sup>2</sup> chat.
4. [ ] Bump the semver of all crates <!-- FAIL-CI: We need some better process here on how to do it exactly -->
5. [ ] Abort the release process and announce so in General if there are no bumps needed.
6. [ ] Create a merge request to `release` with the proposed SemVer bumps.
7. [ ] Announce this merge request in the *General* channel to quickly gather reviews.
8. [ ] Merge it into `release`.
9. [ ] Verify all CI checks.
10. [ ] Announce the intent to do a *Mainline* release from the resulting commit hash in RelEng.
11. [ ] <!-- The release team has internal checklists for QA i think, should we mention this? -->
12. [ ] Release all crates to crates.io.

## Nightly Release

Cadence: every day at 00:00 UTC+1. Responsible: Release Team

This process aims to release the `master` branch as a *Nightly* release every day. The process can start at 00:00 UTC+1 and should automatically do the following steps.

1. [ ] Check out the latest commit of branch `master`.
3. [ ] Compare this commit to the latest `nightly*` tag. Announce that the process was aborted in the RelEng chat since there were no changes.
4. [ ] Verify all CI checks of that commit.
5. [ ] Set the version of all crate to `major.0.0-nightlyYYMMDD` where `major` is the last released `major` version of that crate plus one.
6. [ ] Tag this commit as `nightlyYYMMDD`.
7. [ ] Announce the intent to do a *Nightly* release from that tag in the RelEng chat.
8. [ ] Release all crates to crates.io using [parity-publish](https://github.com/paritytech/parity-publish). <!-- FAIL-CI: I think Morgan fixed that tool so it would only release crates that had changes, or that had one of their transitive dependencies changes. That would help, since otherwise we always push 400 crates or so. -->

## Clobbering

Cadence: every 6th release (~3 months). Responsible: Release Team

This process aims to bring branch `release` in sync with the latest audited commit of `master`. It is not done via a Merge Request but rather by just copying files. It should be automated.

The following script is provided to do the clobbering.

```bash
git fetch
git checkout release
git reset --hard origin/audited
git push --force release
```

# Footnotes

1: `RelEng`: The *RelEng: Polkadot Release Coordination* Matrix channel.  
2: `General`: The *General* Matrix channel.  
3: `SemVer`: Semantic Versioning v2.0.0 as defined on https://semver.org/.
