# Contributing

The `Polkadot SDK` project is an **OPENISH Open Source Project**

## What?

Individuals making significant and valuable contributions are given commit-access to the project. Contributions are done
via pull-requests and need to be approved by the maintainers.

> **Note:** Contributors who are part of the organization do not need to fork the repository. They can create a branch
> directly in the repository to send a pull request.

## How?

In order to build this project you need to install some dependencies, follow the instructions in [this guide](https://docs.polkadot.com/develop/parachains/install-polkadot-sdk).

## Rules

There are a few basic ground-rules for contributors (including the maintainer(s) of the project):

1. **No `--force` pushes** or modifying the master branch history in any way. If you need to rebase, ensure you do it in
   your own repo. No rewriting of the history after the code has been shared (e.g. through a Pull-Request).
2. **Non-master branches**, prefixed with a short name moniker (e.g. `gav-my-feature`) must be used for ongoing work.
3. **All modifications** must be made in a **pull-request** to solicit feedback from other contributors.
4. A pull-request **must not be merged until CI** has finished successfully.
5. Contributors should adhere to the [house coding style](./STYLE_GUIDE.md).
6. Contributors should adhere to the [house documenting style](./DOCUMENTATION_GUIDELINES.md), when applicable.

## Merge Process

### In General

* A Pull Request (PR) needs to be reviewed and approved by project maintainers.
* If a change does not alter any logic (e.g. comments, dependencies, docs), then it may be tagged `A1-insubstantial` and
merged faster.
* No PR should be merged until all reviews' comments are addressed.

### Labels

The set of labels and their description can be found [here](https://paritytech.github.io/labels/doc_polkadot-sdk.html).

### Process

1. Please use our [Pull Request Template](./PULL_REQUEST_TEMPLATE.md) and make sure all relevant information is
   reflected in your PR.
2. Please tag each PR with minimum one `T*` label. The respective `T*` labels should signal the component that was
   changed, they are also used by downstream users to track changes and to include these changes properly into their own
   releases.
3. If you’re still working on your PR, please submit as “Draft”. Once a PR is ready for review change the status to
   “Open”, so that the maintainers get to review your PR. Generally PRs should sit for 48 hours in order to garner
   feedback. It may be merged before if all relevant parties had a look at it.
4. With respect to auditing, please see [AUDIT.md](../AUDIT.md). In general, merging to master can happen independently of
   audit.
5. PRs will be able to be merged once all reviewers' comments are addressed and CI is successful.

**Noting breaking changes:** When breaking APIs, the PR description should mention what was changed alongside some
examples on how to change the code to make it work/compile. It should also mention potential storage migrations and if
they require some special setup aside from adding it to the list of migrations in the runtime.

## Reviewing pull requests

When reviewing a pull request, the end-goal is to suggest useful changes to the author. Reviews should finish with
approval unless there are issues that would result in:
1. Buggy behavior.
2. Undue maintenance burden.
3. Breaking with house coding style.
4. Pessimization (i.e. reduction of speed as measured in the projects benchmarks).
5. Feature reduction (i.e. it removes some aspect of functionality that a significant minority of users rely on).
6. Uselessness (i.e. it does not strictly add a feature or fix a known issue).

The reviewers are also responsible to check:

* if the PR description is well written to facilitate integration, in case it contains breaking changes.
* the PR has an impact on docs.

**Reviews may not be used as an effective veto for a PR because**:
1. There exists a somewhat cleaner/better/faster way of accomplishing the same feature/fix.
2. It does not fit well with some other contributors' longer-term vision for the project.

## `PRDoc`

All Pull Requests must contain proper title & description, as described in [Pull Request
Template](./PULL_REQUEST_TEMPLATE.md). Moreover, all pull requests must have a proper `prdoc` file attached.

Some Pull Requests can be exempt of `prdoc` documentation, those must be labelled with
[`R0-silent`](https://github.com/paritytech/labels/blob/main/ruled_labels/specs_polkadot-sdk.yaml#L95-L97).

Non "silent" PRs must come with documentation in the form of a `.prdoc` file.

See more about `prdoc` [here](./prdoc.md)

## Crate Configuration `Cargo.toml`

The Polkadot SDK uses many conventions when configuring a crate. Watch out for these things when you
are creating a new crate.

### Is the Crate chain-specific?

Chain-specific crates, for example
[`bp-bridge-hub-rococo`](https://github.com/paritytech/polkadot-sdk/blob/4014b9bf2bf8f74862f63e7114e5c78009529be5/bridges/chains/chain-bridge-hub-rococo/Cargo.toml#L10-L11)
, should not be released as part of the Polkadot-SDK umbrella crate. We have a custom metadata
attribute that is picked up by the [generate-umbrella.py](../../scripts/generate-umbrella.py)
script, that should be applied to all chain-specific crates like such:

```toml
[package]
# Other stuff...

[package.metadata.polkadot-sdk]
exclude-from-umbrella = true

# Other stuff...
```

### Is the Crate a Test, Example or Fuzzer?

Test or example crates, like
[`pallet-example-task`](https://github.com/paritytech/polkadot-sdk/blob/9b4acf27b869d7cbb07b03f0857763b8c8cc7566/substrate/frame/examples/tasks/Cargo.toml#L9)
, should not be released to crates.io. To ensure this, you must add `publish = false` to your
crate's `package` section:

```toml
[package]
# Other stuff...

publish = false

# Other stuff...
```

## Helping out

We use [labels](https://github.com/paritytech/polkadot-sdk/labels) to manage PRs and issues and communicate state of a
PR. Please familiarise yourself with them. Best way to get started is to a pick a ticket tagged
[easy](https://github.com/paritytech/polkadot-sdk/issues?q=is%3Aopen+is%3Aissue+label%3AD0-easy) or
[medium](https://github.com/paritytech/polkadot-sdk/issues?q=is%3Aopen+is%3Aissue+label%3AD1-medium) and get going.
Alternatively, look out for issues tagged
[mentor](https://github.com/paritytech/polkadot-sdk/issues?q=is%3Aopen+is%3Aissue+label%3AC1-mentor) and get in contact
with the mentor offering their support on that larger task.

****

### Issues

If what you are looking for is an answer rather than proposing a new feature or fix, search
[https://substrate.stackexchange.com](https://substrate.stackexchange.com/) to see if an post already exists, and ask if
not. Please do not file support issues here.

Before opening a new issue search to see if a similar one already exists and leave a comment that you also experienced
this issue or add your specifics that are related to an existing issue.

Please label issues with the following labels (only relevant for maintainer):
1. `I*`  issue severity and type. EXACTLY ONE REQUIRED.
2. `D*`  issue difficulty, suggesting the level of complexity this issue has. AT MOST ONE ALLOWED.
3. `T*`  Issue topic. MULTIPLE ALLOWED.

## Releases

Declaring formal releases remains the prerogative of the project maintainer(s). See [RELEASE.md](../RELEASE.md).

## UI tests

UI tests are used for macros to ensure that the output of a macro doesn’t change and is in the expected format. These UI
tests are sensible to any changes in the macro generated code or to switching the rust stable version. The tests are
only run when the `RUN_UI_TESTS` environment variable is set. So, when the CI is for example complaining about failing
UI tests and it is expected that they fail these tests need to be executed locally. To simplify the updating of the UI
test output there is a script
* `./scripts/update-ui-tests.sh`   to update the tests for a current rust version locally
* `./scripts/update-ui-tests.sh 1.70` # to update the tests for a specific rust version locally

Or if you have opened PR and you're member of `paritytech` - you can use [/cmd](./commands-readme.md)
to run the tests for you in CI:
* `/cmd update-ui` - will run the tests for the current rust version
* `/cmd update-ui --image docker.io/paritytech/ci-unified:bullseye-1.70.0-2023-05-23` -
will run the tests for the specified rust version and specified image

## Feature Propagation

We use [zepter](https://github.com/ggwpez/zepter) to enforce features are propagated between crates correctly.

## Command Bot

If you're member of **paritytech** org - you can use command-bot to run various of common commands in CI:

Start with comment in PR: `/cmd --help` to see the list of available commands.


## Deprecating code

When deprecating and removing code you need to be mindful of how this could impact downstream developers. In order to
mitigate this impact, it is recommended to adhere to the steps outlined in the [Deprecation
Checklist](./DEPRECATION_CHECKLIST.md).
