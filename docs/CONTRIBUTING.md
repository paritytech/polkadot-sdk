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
3. If your’re still working on your PR, please submit as “Draft”. Once a PR is ready for review change
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

## Releases

Declaring formal releases remains the prerogative of the project maintainer(s).

## UI tests

UI tests are used for macros to ensure that the output of a macro doesn’t change and is in the expected format.
These UI tests are sensible to any changes in the macro generated code or to switching the rust stable version.
The tests are only run when the `RUN_UI_TESTS` environment variable is set. So, when the CI is for example complaining
about failing UI tests and it is expected that they fail these tests need to be executed locally.
To simplify the updating of the UI test ouput there is the `.maintain/update-rust-stable
