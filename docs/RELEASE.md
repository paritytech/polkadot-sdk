# Release

The outputs of a release are the `polkadot` and `polkadot-parachain` node binaries, the runtimes for Westend & Rococo
and their system parachains, and new crate versions published to `crates.io`.

# Setup

We have two branches: `master` and `stable`. `master` is the main development branch where normal Pull Requests are
opened. Developers need to mostly only care about this branch.  
The `stable` branch contains a version of the code that is ready to be released. Its contents are always audited.
Merging to it is restricted to [Backports](#backports).

# Versioning

We are releasing multiple different things from this repository in one release, but we don't want to use the same
version for everything. Thus, in the following we explain the versioning story for the crates, node and Westend &
Rococo. To easily refer to a release, it shall be named by its date in the form `stableYYMMDD`.

## Crate

We try to follow [SemVer 2.0.0](https://semver.org/) as best as possible for versioning our crates. The definitions of
`major`, `minor` and `patch` version for Rust crates are slightly altered from their standard for pre `1.0.0` versions.
Quoting [rust-lang.org](https://doc.rust-lang.org/cargo/reference/semver.html):  

>Initial development releases starting with “0.y.z” can treat changes in “y” as a major release, and “z” as a minor
release. “0.0.z” releases are always major changes. This is because Cargo uses the convention that only changes in the
left-most non-zero component are considered incompatible.

SemVer requires a piece of software to first declare a public API. The public API of the Polkadot SDK
is hereby declared as the sum of all crates' public APIs.

Inductively, the public API of our library crates is declared as all public items that are neither:
- Inside a `__private` module
- Documented as "unstable" or "experimental" in the first line of docs
- Bear `unstable` or `experimental` in their absolute path

## Node

The versioning of the Polkadot node is done most of the time by only incrementing the `minor` version. The `major`
version is only bumped for special releases and the `patch` can be used for an out of band release that fixes some
critical bug. The node version is not following SemVer. This means that the version doesn't express if there are any
breaking changes in the CLI interface or similar. The node version is declared in the
[`NODE_VERSION`](https://paritytech.github.io/polkadot-sdk/master/polkadot_node_primitives/constant.NODE_VERSION.html)
variable.

## Westend & Rococo

For these networks, in addition to incrementing the `Cargo.toml` version we also increment the `spec_version` and
sometimes the `transaction_version`. The spec version is also following the node version. Its schema is: `M_mmm_ppp` and
for example `1_002_000` is the node release `1.2.0`. This versioning has no further meaning, and is only done to map
from an on chain `spec_version` easily to the release in this repository.  
The Westend testnet will be updated to a new runtime every two weeks with the latest `nightly` release.

# Backports

**From `master` to `stable`**

Backports in this direction can be anything that is audited and either a `minor` or a `patch` bump. [Security
fixes](#bug-and-security-fix) should be prioritized over additions or improvements. Crates that are declared as internal
API can also have `major` version bumps through backports.

**From `stable` to `master`**

Should not be needed since all changes first get merged into `master`. The `stable` branch can get out of sync and will
be synced with the [Clobbering](#clobbering) process.

# Processes

The following processes are necessary to actualize our releases. Each process has a *Cadence* on which it must execute
and a *Responsible* that is responsible for autonomously doing so and reporting back any error in the *RelEng: Polkadot
Release Coordination* Matrix channel. All processes should be automated as much as possible.

## Crate Bumping

Cadence: (possibly) each Pull Request. Responsible: Developer that opened the Pull Request.

Following SemVer isn't easy, but there exists [a guide](https://doc.rust-lang.org/cargo/reference/semver.html) in the
Rust documentation that explains the small details on when to bump what. This process is supported with a CI check that
utilizes [`cargo-semver-checks`](https://github.com/obi1kenobi/cargo-semver-checks).

### Steps

1. Developer opens a Pull Request with changed crates against `master`.
1. They bump all changed crates according to SemVer. Note that this includes any crates that expose the changed
   behaviour in their *public API* and also transitive dependencies for whom the same rule applies.

## Stable Release

Cadence: every two weeks. Responsible: Release Team.

This process aims to release the `stable` branch as a *Stable* release every two weeks.

### Steps

1. Check and execute process [Clobbering](#clobbering), if needed.
2. Check if there were any changes since the last release and abort, if not.
3. Check out the latest commit of `stable`.
4. Update the `CHANGELOG.md` version, date and compile the content using the PrDoc files.
5. Open a Pull Request against `stable` for visibility of the release happening.
6. Internal QA from the release team can happen here.
7. Do a dry-run release to ensure that it *should* work.
8. Comment in the Pull Request that a *Stable* release will happen from the merged commit hash.
9. Release all changed crates to crates.io.
10. Create the release `stableYYYYMMDD` on GitHub. Note that the Fellowship has a streamlined process that combines the
    two last steps. A similar approach should be taken here.

## Nightly Release

Cadence: every day at 00:00 UTC+1. Responsible: Release Team

This process aims to release the `master` branch as a *Nightly* release. The process can start at 00:00 UTC+1 and should
automatically do the following steps.

1. Check out the latest commit of branch `master`.
2. Compare this commit to the latest `nightly*` tag and abort if there are no changes detected.
3. Set the version of all crates that changed to `major.0.0-nightlyYYMMDD` where `major` is the last released `major`
   version of that crate plus one.
4. Patch the dependencies of the changed crates to point to the newest version of the dependency.
5. Tag this commit as `nightlyYYMMDD`.
6. Do a dry-run release to ensure that it *should* work.
7. Push this tag (the commit will not belong to any branch).
8. Release all crates that had changed to crates.io.

## Clobbering

Cadence: every 6th release (~3 months). Responsible: Release Team

This process aims to bring branch `stable` in sync with the latest audited commit of `master`. It is not done via a Pull
Request but rather by just copying files. It should be automated.  
The following script is provided to do the clobbering. Note that it keeps the complete history of all past clobbering
processes.

```bash
# Ensure we have the latest remote data
git fetch
# Switch to the stable branch
git checkout stable

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

1. Developer opens a Pull Request with a bug or security fix.
2. The Pull Request is marked as priority fix.
3. Audit happens with priority.
4. It is merged into `master`.
5. It is automatically back-ported to `stable`.
6. The fix will be released in the next *Stable* release. In urgent cases, a release can happen earlier.
