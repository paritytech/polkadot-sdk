# Release

The outputs of a stable release are:

- The binaries:
   - `polkadot`
   - `polkadot-execute-worker`
   - `polkadot-prepare-worker`
   - `polkadot-parachain`
   - `polkadot-omni-node`
   - `chain-spec-builder`
   - `frame-omni-bencher`

   built for the `x86_64-unknown-linux-gnu` and `aarch64-apple-darwin` targets. The gpg signatures and sha256 checksums.

- The runtimes for Westend and its system parachains.
- The new crate versions published to `crates.io`.
- Debian package for the Polkadot binary.
- Docker images for:
   - `polkadot` (includes `polkadot-execute-worker` & `polkadot-prepare-worker`)
   - `polkadot-parachain`
   - `polkadot-omni-node`
   - `chain-spec-builder`


# Timeline
`Stable` releases are scheduled on a quarterly basis, usually by the end of the last month of each quarter. The exact
 schedule can be found on the [Release Registry](https://github.com/paritytech/release-registry/).It is possible to
 subscribe to a [calendar link](https://raw.githubusercontent.com/paritytech/release-registry/main/releases-v1.ics)
 to have it in your personal calendar.

Each stable release is supported for a period of one year from its first release. For example, `Polkadot stable2412`
was released on `2024-12-17` and its end of life is set to `2025-12-16`.

During this period, each stable release is updated with patch releases, which are scheduled on a monthly basis
and contain fixes for any bugs that may be found.

ℹ️ Note: the binaries and runtimes (if needed) are only provided for the latest `stable` release, for the previous
releases only the crates.io release takes place.

This three month period between `stable` releases includes a 1.5 month QA period. This means that for each upcoming
`stable` release, the branch from which that release will be made is created 1.5 months before the release date.
This time is used to test the upcoming release candidate and find any issues that may arise with publishing crates
to crates.io, binary artifacts and template synchronisation before the final release. The findings should be fixed
 and backported to the release branch. During this time, multiple release candidates may be built and rolled out.

# Setup

We have two types of branches related to the releases: `master` and `stableYYMM`:
- `master` is the main development branch where normal Pull Requests are opened. Developers need to mostly only care
about this branch.
- `stableYYMM` branch contains a version of the code that is ready to be released. Each `stableYYMM` branch corresponds
to the corresponding stable release, which is in the maintenance or in a QA period. Its contents should be always
audited. Merging to it is restricted to [Backports](#backports).

# Versioning

We are releasing multiple different things from this repository in one release, but we don't want to use the same
version for everything. Thus, in the following we explain the versioning story for the crates, node and Westend.

To easily refer to a release, it shall be named by its date in the form `Polkadot stableYYMM`. Patches to stable releases
are tagged in the form of `Polkadot stableYYMM-PATCH`, with `PATCH` ranging from 1 to 99. For example, the fourth patch
to `Polkadot stable2409` would be `Polkadot stable2409-4`.

## Crate

We try to follow [SemVer 2.0.0](https://semver.org/) as best as possible for versioning our crates. The definitions of
`major`, `minor` and `patch` version for Rust crates are slightly altered from their standard for pre `1.0.0` versions.
Quoting [rust-lang.org](https://doc.rust-lang.org/cargo/reference/semver.html):

>Initial development releases starting with "0.y.z" can treat changes in "y" as a major release, and "z" as a minor
release. "0.0.z" releases are always major changes. This is because Cargo uses the convention that only changes in the
left-most non-zero component are considered incompatible.

SemVer requires a piece of software to first declare a public API. The public API of the Polkadot SDK
is hereby declared as the sum of all crates' public APIs.

Inductively, the public API of our library crates is declared as all public items that are neither:
- Inside a `__private` module
- Documented as "unstable" or "experimental" in the first line of docs
- Bear `unstable` or `experimental` in their absolute path

## Node

The versioning of the Polkadot node is done most of the time by only incrementing the `minor` version. The `major`
version is only bumped for special releases and the `patch` is used for a patch release that happens every month and
fixes found issues. The node version is not following SemVer. This means that the version doesn't express if there are
any breaking changes in the CLI interface or similar. The node version is declared
in the [`NODE_VERSION`](https://paritytech.github.io/polkadot-sdk/master/polkadot_node_primitives/constant.NODE_VERSION.html)
variable.

## Westend

For the Westene testnet, in addition to incrementing the `Cargo.toml` version we also increment the `spec_version` and
sometimes the `transaction_version`. The spec version is also following the node version. Its schema is: `M_mmm_ppp` and
for example `1_002_000` is the node release `1.2.0`. This versioning has no further meaning, and is only done to map
from an on chain `spec_version` easily to the release in this repository.


# Backports

**From `master` to `stable`**

Backports in this direction can be anything that is audited and either a `minor` or a `patch` bump.
See [BACKPORT.md](./BACKPORT.md) for more explanation. [Security fixes](#bug-and-security-fix)
should be prioritized over additions or improvements. Crates that are declared as internal API can
also have `major` version bumps through backports.

**From `stable` to `master`**

Backports to `master` only happen after a `stable` or `patch` release has been made for the current stable release,
and include node version and spec version bumps, plus reorganizing the prdoc files
(they should go into the appropriate release folder under the [prdoc](./proc) folder).
This is done by the release team to keep things organized. Developers need not to care about such backports.

# Processes

The following processes are necessary to actualize our releases. Each process has a *Cadence* on which it must execute
and a *Responsible* that is responsible for autonomously doing so and reporting back any error in the *RelEng: Polkadot
Release Coordination* Matrix channel. All processes should be automated as much as possible.

## Crate Bumping

Cadence: currently every 3 months for new `stable` releases and monthly for existing `stables`.
Responsible: Developer that opened the Pull Request.

Following SemVer isn't easy, but there exists [a guide](https://doc.rust-lang.org/cargo/reference/semver.html) in the
Rust documentation that explains the small details on when to bump what. This process is supported with a CI check that
utilizes [`cargo-semver-checks`](https://github.com/obi1kenobi/cargo-semver-checks).

### Steps

1. Developer opens a Pull Request with changed crates against `master`.
2. They mention the type of the bump of all changed crates according to SemVer in the prdoc file attached to the PR.
 Note that this includes any crates that expose the changed behaviour in their *public API* and also transitive dependencies
 for whom the same rule applies.
3. The bump itself happens during the release and is done by a release engineer using the
[Parity-Publish](https://github.com/paritytech/parity-publish) tool.

## Stable Release

Cadence: every 3 months for new `stable` releases and monthly for existing `stables`. Responsible: Release Team.

### Steps to execute a new stable release

From the main Polkadot-sdk repository in the paritytech org:

1. On the cut-off date, create a new branch with the name `satbleYYMM`
using [Branch-off stable flow](/.github/workflows/release-10_branchoff-stable.yml)
2. Create a new rc tag from the stable branch using [RC Automation flow](/.github/workflows/release-11_rc-automation.yml)

From the forked Polkadot-sdk repository in the [paritytech-release org](https://github.com/paritytech-release/polkadot-sdk/actions):

1. Sync the forks before continuing with the release using
[Sync the forked repo with the upstream](https://github.com/paritytech-release/polkadot-sdk/actions/workflows/fork-sync-action.yml)
2. To build binaries trigger [Release - Build node release candidate](/.github/workflows/release-20_build-rc.yml)
3. When an rc build is ready to trigger [Release - Publish draft](/.github/workflows/release-30_publish_release_draft.yml)
to create a new release draft for the upcoming rc
4. When the release is finalized and ready to go, publish crates using `parity-publish` tool and push changes
to the release branch
5. Repeat steps 1-3 to prepare the rc
6. Trigger [Release - Promote RC to final candidate on S3](/.github/workflows/release-31_promote-rc-to-final.yml)
to have it as a final rc on the S3
7. Publish deb package for the `polkadot` binary using
[Release - Publish Polkadot deb package](/.github/workflows/release-40_publish-deb-package.yml)
8. Adjust the release draft and publish release on the GitHub.
9. Publish docker images using [Release - Publish Docker Image](/.github/workflows/release-50_publish-docker.yml)

From the main Polkadot-sdk repository in the paritytech org:

1. Synchronize templates using [Synchronize templates](/.github/workflows/misc-sync-templates.yml)
2. Update the [Release Registry](https://github.com/paritytech/release-registry/)
follwoing the [instructions](https://github.com/paritytech/release-registry?tab=readme-ov-file#maintenance)
in the repo with the actual release dates.

## Patch release for the latest stable version

Cadence: every month. Responsible: Developer

Describes how developers should merge bug and security fixes.

### Steps

1. Developer opens a Pull Request with a bug or security fix.
2. The Pull Request is marked as priority fix.
3. Audit happens with priority.
4. It is merged into `master`.
5. Dev adds the `A4-needs-backport` label.
6. It is automatically back-ported to `stable` and merged by a release engineer.
7. The fix will be released in the next *Stable patch* release. In urgent cases, a release can happen earlier.

The release itself is similar to [the new stable release](#steps-to-execute-a-new-stable-release) process without
the branching-off step, as the branch already exists and depending on the patch
(whether it is for the current `stable` release or one of the previous ones) the binary build can be skipped
and only crates and GitHub publishing is done.
