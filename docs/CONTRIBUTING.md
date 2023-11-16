# Contributing

The `Polkadot SDK` project is an **OPENISH Open Source Project**

## What?

Individuals making significant and valuable contributions are given commit-access to the project.
Contributions are done via pull-requests and need to be approved by the maintainers.

## Rules

There are a few basic ground-rules for contributors (including the maintainer(s) of the project):

1. **No `--force` pushes** or modifying the master branch history in any way.
   If you need to rebase, ensure you do it in your own repo. No rewriting of the history
   after the code has been shared (e.g. through a Pull-Request).
2. **Non-master branches**, prefixed with a short name moniker (e.g. `gav-my-feature`) must be
   used for ongoing work.
3. **All modifications** must be made in a **pull-request** to solicit feedback from other contributors.
4. A pull-request **must not be merged until CI** has finished successfully.
5. Contributors should adhere to the [house coding style](./STYLE_GUIDE.md).
6. Contributors should adhere to the [house documenting style](./DOCUMENTATION_GUIDELINES.md), when applicable.

## Merge Process

### In General

A Pull Request (PR) needs to be reviewed and approved by project maintainers.
If a change does not alter any logic (e.g. comments, dependencies, docs), then it may be tagged
`A1-insubstantial` and merged faster.
If it is an urgent fix with no large change to logic, then it may be merged after a non-author
contributor has reviewed it well and approved the review once CI is complete.
No PR should be merged until all reviews' comments are addressed. It is expected that 
there is no PR merged that is not ready. It is only allowed to merge such a PR if 
the code is disabled, not used or in any other way clearly tagged as experimental. 
It is expected that master is always releasable and that all PRs got tested well 
enough that the likelihood of critical bugs is quite small.

### Labels

The set of labels and their description can be found [here](https://paritytech.github.io/labels/doc_polkadot-sdk.html).

### Process

1. Please use our [Pull Request Template](./PULL_REQUEST_TEMPLATE.md) and make sure all relevant
   information is reflected in your PR.
2. Please tag each PR with minimum one `T*` label. The respective `T*` labels should signal the
   component that was changed, they are also used by downstream users to track changes and to
   include these changes properly into their own releases.
3. If you’re still working on your PR, please submit as “Draft”. Once a PR is ready for review change
   the status to “Open”, so that the maintainers get to review your PR. Generally PRs should sit for
   48 hours in order to garner feedback. It may be merged before if all relevant parties had a look at it.
4. If you’re introducing a major change, that might impact the documentation please add the label
   `T13-documentation`. The docs team will get in touch.
5. If your PR changes files in these paths:

   `polkadot` : `^primitives/src/`
   `polkadot` : `^runtime/common`
   `substrate` : `^frame/`
   `substrate` : `^primitives/`

   It should be added to the [security audit board](https://github.com/orgs/paritytech/projects/103)
   and will need to undergo an audit before merge.
6. PRs will be able to be merged once all reviewers' comments are addressed and CI is successful.

**Noting breaking changes:**
When breaking APIs, the PR description should mention what was changed alongside some examples on how
to change the code to make it work/compile.
It should also mention potential storage migrations and if they require some special setup aside adding
it to the list of migrations in the runtime.

## Reviewing pull requests

When reviewing a pull request, the end-goal is to suggest useful changes to the author.
Reviews should finish with approval unless there are issues that would result in:
1. Buggy behavior.
2. Undue maintenance burden.
3. Breaking with house coding style.
4. Pessimization (i.e. reduction of speed as measured in the projects benchmarks).
5. Feature reduction (i.e. it removes some aspect of functionality that a significant minority of users rely on).
6. Uselessness (i.e. it does not strictly add a feature or fix a known issue).

The reviewers are also responsible to check:

1. if a changelog is necessary and attached
1. the quality of information in the changelog file
1. the PR has an impact on docs
1. that the docs team was included in the review process of a docs update

**Reviews may not be used as an effective veto for a PR because**:
1. There exists a somewhat cleaner/better/faster way of accomplishing the same feature/fix.
2. It does not fit well with some other contributors' longer-term vision for the project.

## Documentation

All Pull Requests must contain proper title & description.

Some Pull Requests can be exempt of `prdoc` documentation, those
must be labelled with
[`R0-silent`](https://github.com/paritytech/labels/blob/main/ruled_labels/specs_polkadot-sdk.yaml#L89-L91).

Non "silent" PRs must come with documentation in the form of a `.prdoc` file.
A `.prdoc` documentation is made of a text file (YAML) named `/prdoc/pr_NNNN.prdoc` where `NNNN` is the PR number.
For convenience, those file can also contain a short description/title: `/prdoc/pr_NNNN_pr-foobar.prdoc`.

The CI automation checks for the presence and validity of a `prdoc` in the `/prdoc` folder.
Those files need to comply with a specific [schema](https://github.com/paritytech/prdoc/blob/master/schema_user.json). It
is highly recommended to [make your editor aware](https://github.com/paritytech/prdoc#schemas) of the schema as it is
self-described and will assist you in writing correct content.

This schema is also embedded in the
[prdoc](https://github.com/paritytech/prdoc) utility that can also be used to generate and check the validity of a
`prdoc` locally.

## Helping out

We use [labels](https://github.com/paritytech/polkadot-sdk/labels) to manage PRs and issues and communicate
state of a PR. Please familiarise yourself with them. Best way to get started is to a pick a ticket tagged
[easy](https://github.com/paritytech/polkadot-sdk/issues?q=is%3Aopen+is%3Aissue+label%3AD0-easy)
or [medium](https://github.com/paritytech/polkadot-sdk/issues?q=is%3Aopen+is%3Aissue+label%3AD1-medium)
and get going. Alternatively, look out for issues tagged [mentor](https://github.com/paritytech/polkadot-sdk/issues?q=is%3Aopen+is%3Aissue+label%3AC1-mentor)
and get in contact with the mentor offering their support on that larger task.

****

### Issues

If what you are looking for is an answer rather than proposing a new feature or fix, search
[https://substrate.stackexchange.com](https://substrate.stackexchange.com/) to see if an post already
exists, and ask if not. Please do not file support issues here.
Before opening a new issue search to see if a similar one already exists and leave a comment that you
also experienced this issue or add your specifics that are related to an existing issue.
Please label issues with the following labels:
1. `I*`  issue severity and type. EXACTLY ONE REQUIRED.
2. `D*`  issue difficulty, suggesting the level of complexity this issue has. AT MOST ONE ALLOWED.
3. `T*`  Issue topic. MULTIPLE ALLOWED.

## Release Process

We are aiming for a **two week** release process of the `polkadot-sdk` repository. The output of a release
are the Parity `polkadot` node implementation, the runtimes for the Westend & Rococo networks and new versions 
of the crates. Given the cadence of two weeks, there is no need to halt a release process for any kind 
of PR (no BRAKES for the release train). There is only to be made an exception for high security issues 
for code that is already running in production. However, these critical bug fixes may warrant an out of 
band release anyway, but this should be decided based on the severity on a case by case basis.

### Versioning

We are releasing multiple different things from this repository in one release, but 
we don't want to use the same version for everything. Thus, in the following we explain
the versioning story for the node, Westend & Rococo and the crates. The version associated
to a particular release is taken from the node version.

#### Node

The versioning of the node is done 99% of the time by only incrementing the `minor` version. 
The `major` version is only bumped for special releases and the `patch` can be used for an 
out of band release that fixes some critical bug. The node version is not following SemVer. 
This means that the version doesn't express if there are any breaking changes in the CLI 
interface or similar. The node version is declared in the `NODE_VERSION` variable in 
`polkadot/node/primitives/src/lib.rs`.

#### Westend & Rococo

For the test networks we only increment the `spec_version`. The spec version is also following
the node version. So, `10020` is for example the node release `1.2.0`. This versioning has no
further meaning and is only done to map from an on chain `spec_version` easily to the 
release in this repository.

#### Crate

We want to follow SemVer as best as possible when it comes to crates versioning.
Following SemVer isn't that easy, but there exists [a guide](https://doc.rust-lang.org/cargo/reference/semver.html) 
in the Rust documentation that explains the small details on when to bump what. The bumping 
should be done in between the releases in the PRs. However, it isn't required to 
bump e.g. the `patch` version multiple times in between two releases. There
exists a CI job that checks that the versions are not bumped multiple times to help 
the developer. Another CI job is also checking for SemVer breaking changes. It is using
[`cargo-semver-checks`](https://github.com/obi1kenobi/cargo-semver-checks). While 
the tool isn't perfect, it should help to remind the developer of checking the SemVer 
compatibility of its changes. In general there is not any guarantee to downstream 
that there isn't a breaking in between and thus a `major` (or `minor` on `<1.0.0`) 
bump. If possible, it should be prevented, but we also don't want to carry
"dead code" with us for too long. As long as there are clear instructions on how 
to integrate the breaking changes it should be fine to break things.

So, the general approach is that developers are required to bump the versions in 
all crates that they are changing. If there is a `major` (or `minor` on `<1.0.0`)
crate version bump, it is important to also bump any crate that depends on this
bumped version crate and re-export it. However, if the re-export is done in a
`__private` module (that makes clear that it is internal api) the `major`/`minor`
version bump doesn't ripple and only requires a `minor`/`patch` bump.

### Backports

Backports should most of the time not be required. We should only backport critical 
bug fixes and then release the fixed crates. There should be no need to backport 
anything from a release branch. Bumping the `NODE_VERSION` and `spec_version` of 
the test networks should be done before the release process is started on master.
The only other change on release branches, new weights are not that important to 
be backported.

When a backport is required for some previous release, it is the job of the
developer (assuming it is some internal person) that has created the initial PR
to create the backports. After the backports are done it is important to ensure
that the crate release is being done. We should backport fixes to the releases
of the last 6 months.

## UI tests

UI tests are used for macros to ensure that the output of a macro doesn’t change and is in the expected format.
These UI tests are sensible to any changes in the macro generated code or to switching the rust stable version.
The tests are only run when the `RUN_UI_TESTS` environment variable is set. So, when the CI is for example complaining
about failing UI tests and it is expected that they fail these tests need to be executed locally.
To simplify the updating of the UI test output there is a script
- `./scripts/update-ui-tests.sh`   to update the tests for a current rust version locally
- `./scripts/update-ui-tests.sh 1.70` # to update the tests for a specific rust version locally

Or if you have opened PR and you're member of `paritytech` - you can use command-bot to run the tests for you in CI:
- `bot update-ui` - will run the tests for the current rust version
- `bot update-ui latest --rust_version=1.70.0` - will run the tests for the specified rust version
- `bot update-ui latest -v CMD_IMAGE=paritytech/ci-unified:bullseye-1.70.0-2023-05-23 --rust_version=1.70.0` -
will run the tests for the specified rust version and specified image

## Command Bot

If you're member of **paritytech** org - you can use command-bot to run various of common commands in CI:

Start with comment in PR: `bot help` to see the list of available commands.
