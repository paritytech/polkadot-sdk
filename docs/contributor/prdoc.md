# PRDoc

## Intro

With the merge of [PR #1946](https://github.com/paritytech/polkadot-sdk/pull/1946), a new method for
documenting changes has been introduced: `prdoc`. The [prdoc repository](https://github.com/paritytech/prdoc)
contains more documentation and tooling.

The current document describes how to quickly get started authoring `PRDoc` files.

## Requirements

When creating a PR, the author needs to decides with the `R0` label whether the change (PR) should
appear in the release notes or not.

Labelling a PR with `R0` means that no `PRDoc` is required.

A PR without the `R0` label **does** require a valid `PRDoc` file to be introduced in the PR.

## PRDoc how-to

A `.prdoc` file is a YAML file with a defined structure (ie JSON Schema).

For significant changes, a `.prdoc` file is mandatory and the file must meet the following
requirements:
- file named `pr_NNNN.prdoc` where `NNNN` is the PR number.
  For convenience, those file can also contain a short description: `pr_NNNN_foobar.prdoc`.
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

## `prdoc` cli usage

The `prdoc` cli documentation can be found at https://github.com/paritytech/prdoc#prdoc

tldr:
- `prdoc generate <NNNN>`
- `prdoc check -n <NNNN>`

where <NNNN> is the PR number.

## Pick an audience

While describing a PR, the author needs to consider which audience(s) need to be addressed.
The list of valid audiences is described and documented in the JSON schema as follow:

- `Node Dev`: Those who build around the client side code. Alternative client builders, SMOLDOT, those who consume RPCs.
   These are people who are oblivious to the runtime changes. They only care about the meta-protocol, not the protocol
   itself.

- `Runtime Dev`: All of those who rely on the runtime. A parachain team that is using a pallet. A DApp that is using a
   pallet. These are people who care about the protocol (WASM), not the meta-protocol (client).

- `Node Operator`: Those who don't write any code and only run code.

- `Runtime User`: Anyone using the runtime. This can be a token holder or a dev writing a front end for a chain.

## Tips

The PRDoc schema is defined in each repo and usually is quite restrictive.
You cannot simply add a new property to a `PRDoc` file unless the Schema allows it.
