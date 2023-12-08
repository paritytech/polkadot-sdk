# Release

The outputs of a release are the `polkadot` and `polkadot-parachain` nodes, runtimes for the Westend & Rococo networks, including their system parachains, and new crate versions published to `crates.io`.

## Versioning

We are releasing multiple different things from this repository in one release, but 
we don't want to use the same version for everything. Thus, in the following we explain
the versioning story for the crates, node and Westend & Rococo. To easily refer to a release, we shall use the node version of it.

### Crate

We try to follow SemVer<sup>3</sup> as best as possible for versioning our crates' public APIs.  

👉 The public API of our library crates is defined as all public items that are not inside a `__private` module.

### Node

The versioning of the node is done most of the time by only incrementing the `minor` version. 
The `major` version is only bumped for special releases and the `patch` can be used for an 
out of band release that fixes some critical bug. The node version is not following SemVer. 
This means that the version doesn't express if there are any breaking changes in the CLI 
interface or similar. The node version is declared in the `NODE_VERSION` variable in 
`polkadot/node/primitives/src/lib.rs`.

### Westend & Rococo

For the these networks, in addition to incrementing the Cargo.toml version we also increment the `spec_version` and sometimes the `transaction_version`. The spec version is also following
the node version. Its schema is: `M_mmm_ppp` and for example `1_002_000` is the node release `1.2.0`. This versioning has no further meaning, and is only done to map from an on chain `spec_version` easily to the release in this repository. 

## Backports

Backports should be rare. We should only backport [critical bug fixes](#bug-and-security-fix) and then release the fixed crates. There should be no need to backport anything from a release branch.

# Processes

The following processes are necessary to actualize our releases. Each process has a *Cadence* on which it must execute and an *Responsible* that is responsible for autonomously doing so and reporting back any error in the RelEng<sup>1</sup> channel.

## Crate Bumping

Cadence: (possibly) each Merge Request. Responsible: Developer that opened the MR.

Following SemVer isn't easy, but there exists [a guide](https://doc.rust-lang.org/cargo/reference/semver.html) in the Rust documentation that explains the small details on when to bump what. This process should be augmented with CI checks that utilize [`cargo-semver-checks`](https://github.com/obi1kenobi/cargo-semver-checks) and/or [`cargo-public-api`](https://github.com/Enselic/cargo-public-api). They must also pay attention to downstream dependencies that require a version bump, because they export the changed API.

### Steps

1. Developer opens a Merge Request with changed crates against `master`.
2. They bump all changed crates according to SemVer.
3. They bump all crates that export any changed types in their *public API*.
4. They also bump all crates that inherit logic changes from relying on one of the bumped crates. 

## Mainline Release

Cadence: every two weeks. Responsible: Release Team.

This process aims to release the `release` branch as a *Mainline* release every two weeks. It should eventually be automated.

### Steps

1. Check if process [Clobbering](#clobbering) needs to happen and do so, if that is the case.
2. Check out the latest commit of `release`.
3. Verify all CI checks of that commit.
4. Update the `CHANGELOG.md` version and date. <!-- Here we could aggregate the PrDocs into the CHANGELOG -->
5. Open a Merge Request against `release` for visibility.
6. Check if there were any changes since the last release and abort, if not.
7. Run `cargo semver-checks` and `cargo public-api` again to ensure that there are no SemVer breaks.
8. Internal QA from the release team can happen here.
9. Do a dry-run release to ensure that it *should* work.
10. Merge it into `release`.
11. Verify all CI checks.
12. Comment that a *Mainline* release will happen from the merged commit hash.
13. Release all changed crates to crates.io.
14. Create a release on GitHub.

## Nightly Release

Cadence: every day at 00:00 UTC+1. Responsible: Release Team

This process aims to release the `master` branch as a *Nightly* release. The process can start at 00:00 UTC+1 and should automatically do the following steps.

1. Check out the latest commit of branch `master`.
2. Verify all CI checks of that commit.
3. Compare this commit to the latest `nightly*` tag and abort if there are no changes detected.
4. Set the version of all crates to `major.0.0-nightlyYYMMDD` where `major` is the last released `major` version of that crate plus one.
5. Tag this commit as `nightlyYYMMDD`.
9. Do a dry-run release to ensure that it *should* work.
7. Push this tag (the commit will not belong to any branch).
8. Announce the intent to do a *Nightly* release from that tag in the RelEng chat.
9. Release all crates that had changed since the last nightly release to crates.io.
10. Create a release on GitHub.

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

## Bug and Security Fix

Cadence: n.a. Responsible: Developer

Describes how developers should merge bug and security fixes.

### Steps

1. Developer opens a Merge Request with a bug or security fix.
2. They have the possibility to mark the MR as such, and does so.
3. Audit happens with priority.
4. It is merged into `master`.
5. It is automatically back-ported to `release`.
6. The fix will be released in the next *Mainline* release. In urgent cases, a release can happen earlier.

# Footnotes

1: `RelEng`: The *RelEng: Polkadot Release Coordination* Matrix channel.  
2: `General`: The *General* Matrix channel.  
3: `SemVer`: Semantic Versioning v2.0.0 as defined on https://semver.org/.
