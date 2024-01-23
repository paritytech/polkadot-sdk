# Release

The outputs of a release are the `polkadot` and `polkadot-parachain` node binaries, the runtimes for Westend & Rococo and their system parachains, and new crate versions published to `crates.io`.

# Setup

We have two branches: `master` and `stable`. `master` is the main development branch where normal merge requests are opened. Developers need to mostly only care about this branch.  
The `stable` branch contains a version of the code that is ready to be released. Its contents are always audited. Merging to it is restricted to [Backports](#backports).

# Versioning

We are releasing multiple different things from this repository in one release, but we don't want to
use the same version for everything. Thus, in the following we explain the versioning story for the
crates, node and Westend & Rococo. To easily refer to a release, it shall be named by its date in
the form `stableYYMMDD`.

## Crate

We try to follow [SemVer 2.0.0](https://semver.org/) as best as possible for versioning our crates. SemVer requires a piece of software to first declare a public API. The public API of the Polkadot SDK is hereby declared as the sum of all crates' public APIs.


Inductively, the public API of our library crates is declared as all public items that are neither:
- Inside a `__private` module
- Documented as "unstable" or "experimental" in the first line of docs
- Bear `unstable` or `experimental` in their absolute path

## Node

The versioning of the Polkadot node is done most of the time by only incrementing the `minor` version. 
The `major` version is only bumped for special releases and the `patch` can be used for an 
out of band release that fixes some critical bug. The node version is not following SemVer. 
This means that the version doesn't express if there are any breaking changes in the CLI 
interface or similar. The node version is declared in the [`NODE_VERSION`](https://paritytech.github.io/polkadot-sdk/master/polkadot_node_primitives/constant.NODE_VERSION.html) variable.

## Westend & Rococo

For the these networks, in addition to incrementing the `Cargo.toml` version we also increment the
`spec_version` and sometimes the `transaction_version`. The spec version is also following the node
version. Its schema is: `M_mmm_ppp` and for example `1_002_000` is the node release `1.2.0`. This
versioning has no further meaning, and is only done to map from an on chain `spec_version` easily to
the release in this repository.  
The Westend testnet will be updated to the new runtime version immediately after a *Stable* release happened.

# Backports

**From `master` to `stable`**

Backports in this direction can be anything that is audited and either `minor` or a `patch` bump. [Security fixes](#bug-and-security-fix) should be prioritized over additions or improvements.

**From `stable` to `master`**

Should not be needed since all changes first get merged into `master`. The `stable` branch can get out of sync and will be synced with the [Clobbering](#clobbering) process.

# Processes

The following processes are necessary to actualize our releases. Each process has a *Cadence* on which it must execute and a *Responsible* that is responsible for autonomously doing so and reporting back any error in the RelEng<sup>2</sup> channel. All processes should be automated as much as possible.

## Crate Bumping

Cadence: (possibly) each Merge Request. Responsible: Developer that opened the MR.

Following SemVer isn't easy, but there exists [a guide](https://doc.rust-lang.org/cargo/reference/semver.html) in the Rust documentation that explains the small details on when to bump what. This process should be augmented with CI checks that utilize [`cargo-semver-checks`](https://github.com/obi1kenobi/cargo-semver-checks) and/or [`cargo-public-api`](https://github.com/Enselic/cargo-public-api). They must also pay attention to downstream dependencies that require a version bump, because they export the changed API.

### Steps

1. Developer opens a Merge Request with changed crates against `master`.
2. They bump all changed crates according to SemVer.
3. They bump all crates that export any changed types in their *public API*.
4. They also bump all crates that inherit logic changes from relying on one of the bumped crates. 

## Stable Release

Cadence: every two weeks. Responsible: Release Team.

This process aims to release the `stable` branch as a *Stable* release every two weeks.

### Steps

1. Check if process [Clobbering](#clobbering) needs to happen and do so, if that is the case.
2. Check out the latest commit of `stable`.
3. Update the `CHANGELOG.md` version, date and compile the content using the prdoc files.
4. Open a Merge Request against `stable` for visibility.
5. Check if there were any changes since the last release and abort, if not.
6. Run `cargo semver-checks` and `cargo public-api` again to ensure that there are no SemVer breaks.
7. Internal QA from the release team can happen here.
8. Do a dry-run release to ensure that it *should* work.
10. Comment that a *Stable* release will happen from the merged commit hash.
11. Release all changed crates to crates.io.
12. Create a release on GitHub.
13. Notify Devops so that they can update Westend to the new runtime.

## Nightly Release

Cadence: every day at 00:00 UTC+1. Responsible: Release Team

This process aims to release the `master` branch as a *Nightly* release. The process can start at 00:00 UTC+1 and should automatically do the following steps.

1. Check out the latest commit of branch `master`.
2. Compare this commit to the latest `nightly*` tag and abort if there are no changes detected.
3. Set the version of all crates to `major.0.0-nightlyYYMMDD` where `major` is the last released `major` version of that crate plus one.
4. Tag this commit as `nightlyYYMMDD`.
5. Do a dry-run release to ensure that it *should* work.
6. Push this tag (the commit will not belong to any branch).
8. Release all crates that had changed since the last nightly release to crates.io.

## Clobbering

Cadence: every 6th release (~3 months). Responsible: Release Team

This process aims to bring branch `stable` in sync with the latest audited commit of `master`. It is not done via a Merge Request but rather by just copying files. It should be automated.  
The following script is provided to do the clobbering. Note that it keeps the complete history of all past clobbering processes.

```bash
# Ensure we have the latest remote data
git fetch
# Switch to the release branch
git checkout release

# Delete all tracked files in the working directory
git ls-files -z | xargs -0 rm -f
# Find and delete any empty directories
find . -type d -empty -delete

# Get the last audited commit
AUDITED=$(git rev-parse --short=10 origin/audited)
# Grab the files from the commit
git checkout $AUDITED -- .

# Stage, commit, and push the working directory which now matches 'audited' 1:1
git add .
git commit -m "Clobbering with audited ($AUDITED)"
git push
```

## Bug and Security Fix

Cadence: n.a. Responsible: Developer

Describes how developers should merge bug and security fixes.

### Steps

1. Developer opens a Merge Request with a bug or security fix.
2. They have the possibility to mark the MR as such, and does so.
3. Audit happens with priority.
4. It is merged into `master`.
5. It is automatically back-ported to `stable`.
6. The fix will be released in the next *Stable* release. In urgent cases, a release can happen earlier.

# Footnotes

1: `SemVer`: Semantic Versioning v2.0.0 as defined on https://semver.org/.  
2: `RelEng`: The *RelEng: Polkadot Release Coordination* Matrix channel.  
