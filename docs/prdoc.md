# PRDoc

## Intro

With the merge of [PR #1946](https://github.com/paritytech/polkadot-sdk/pull/1946), a new method for documenting changes has been introduced: `prdoc`. The [prdoc repository](https://github.com/paritytech/prdoc) contains more documentation and tooling.

The current document describes how to quickly get started authoring `PRDoc` files.

## Requirements

When creating a PR, the author needs to decides with the `R0` label whether the change (PR) should appear in the release notes or not.

Labelling a PR with `R0` means that no `PRDoc` is required.

A PR without the `R0` label **does** require a valid `PRDoc` file to be introduced in the PR.

## PRDoc how-to

A `.prdoc` file is a YAML file with a defined structure (ie JSON Schema).

For significant changes, a `.prdoc` file is mandatory and the file must meet the following requirements:
- file named `pr_NNNN.prdoc` where `NNNN` is the PR number. For convenience, those file can also contain a short description: `pr_NNNN_foobar.prdoc`.
- located under the [`prdoc` folder](https://github.com/paritytech/polkadot-sdk/tree/master/prdoc) of the repository
- compliant with the [JSON schema](https://json-schema.org/) defined in `prdoc/schema_user.json`

Those requirements can be fulfilled manually without any tooling but a text editor.

## Tooling

Users might find the following helpers convenient:
- Setup VSCode to be aware of the prdoc schema: see [using VSCode](https://github.com/paritytech/prdoc#using-vscode)
- Using the `prdoc` cli to:
  - generate a `PRDoc` file from a [template defined in the Polkadot SDK
    repo](https://github.com/paritytech/polkadot-sdk/blob/master/prdoc/.template.prdoc) simply providing a PR number
  - check the validity of one or more `PRDoc` files

## Tips

The PRDoc schema is defined in each repo and usually is quite restrictive.
You cannot simply add a new property to a `PRDoc` file unless the Schema allows it.

There are however a few convenience optional properties that could be useful to authors:
- `authors`: An array of authors (strings). That can help an author find their PRs easily
- `tags`: Array of strings. Those are unrelated to the Github Labels and provided for authors to flag their changes as  they wish, in order to find PR and Documentation quicker
