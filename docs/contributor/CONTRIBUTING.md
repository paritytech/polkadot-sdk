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
No PR should be merged until all reviews' comments are addressed.

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

   `polkadot` : `^runtime/polkadot`
   `polkadot` : `^runtime/kusama`
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

Some Pull Requests can be exempt of `prdoc` documentation, those must be labelled with
[`R0-silent`](https://github.com/paritytech/labels/blob/main/ruled_labels/specs_polkadot-sdk.yaml#L89-L91).

Non "silent" PRs must come with documentation in the form of a `.prdoc` file.

See more about `prdoc` [here](./prdoc.md)

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

## Releases

Declaring formal releases remains the prerogative of the project maintainer(s).

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

## Feature Propagation

We use [zepter](https://github.com/ggwpez/zepter) to enforce features are propagated between crates correctly.

## Command Bot

If you're member of **paritytech** org - you can use command-bot to run various of common commands in CI:

Start with comment in PR: `bot help` to see the list of available commands.


## Deprecating code

When deprecating and removing code you need to be mindful of how this could impact downstream developers. In order to
mitigate this impact, it is recommended to adhere to the steps outlined in the [Deprecation Checklist](./DEPRECATION_CHECKLIST.md).
