# PRDoc

A [prdoc](https://github.com/paritytech/prdoc) is like a changelog but for a Pull Request. We use this approach to
record changes on a crate level. This information is then processed by the release team to apply the correct crate
version bumps and to generate the CHANGELOG of the next release.

## Requirements

When creating a PR, the author needs to decide with the `R0-silent` label whether the PR has to contain a prdoc. The
`R0` label should only be placed for No-OP changes like correcting a typo in a comment or CI stuff. If unsure, ping
the [CODEOWNERS](../../.github/CODEOWNERS) for advice.

## PRDoc How-To

A `.prdoc` file is a YAML file with a defined structure (ie JSON Schema). Please follow these steps to generate one:

1. Install the [`prdoc` CLI](https://github.com/paritytech/prdoc) by running `cargo install prdoc`.
1. Open a Pull Request and get the PR number.
1. Generate the file with `prdoc generate <PR_NUMBER>`. The output filename will be printed.
1. Optional: Install the `prdoc/schema_user.json` schema in your editor, for example
[VsCode](https://github.com/paritytech/prdoc?tab=readme-ov-file#schemas).
1. Edit your `.prdoc` file according to the [Audience](#pick-an-audience) and [SemVer](#record-semver-changes) sections.
1. Check your prdoc with `prdoc check -n <PR_NUMBER>`. This is optional since the CI will also check it.

> **Tip:** GitHub CLI and jq can be used to provide the number of your PR to generate the correct file:  
> `prdoc generate $(gh pr view --json number | jq '.number') -o prdoc`

## Pick An Audience

While describing a PR, the author needs to consider which audience(s) need to be addressed.
The list of valid audiences is described and documented in the JSON schema as follow:

- `Node Dev`: Those who build around the client side code. Alternative client builders, SMOLDOT, those who consume RPCs.
   These are people who are oblivious to the runtime changes. They only care about the meta-protocol, not the protocol
   itself.

- `Runtime Dev`: All of those who rely on the runtime. A parachain team that is using a pallet. A DApp that is using a
   pallet. These are people who care about the protocol (WASM), not the meta-protocol (client).

- `Node Operator`: Those who don't write any code and only run code.

- `Runtime User`: Anyone using the runtime. This can be a token holder or a dev writing a front end for a chain.

If you have a change that affects multiple audiences, you can either list them all, or write multiple sections and
re-phrase the changes for each audience.

## Record SemVer Changes

All published crates that got modified need to have an entry in the `crates` section of your `PRDoc`. This entry tells
the release team how to bump the crate version prior to the next release. It is very important that this information is
correct, otherwise it could break the code of downstream teams.

The bump can either be `major`, `minor`, `patch` or `none`. The three first options are defined by
[rust-lang.org](https://doc.rust-lang.org/cargo/reference/semver.html), whereas `None` should be picked if no other
applies. The `None` option is equivalent to the `R0-silent` label, but on a crate level. Experimental and private APIs
are exempt from bumping and can be broken at any time. Please read the [Crate Section](../RELEASE.md) of the RELEASE doc
about them.

> **Note**: There is currently no CI in place to sanity check this information, but should be added soon.

### Example

For example when you modified two crates and record the changes:

```yaml
crates:
- name: frame-example
  bump: major
- name: frame-example-pallet
  bump: minor
```

It means that downstream code using `frame-example-pallet` is still guaranteed to work as before, while code using
`frame-example` might break.

### Dependencies

A crate that depends on another crate will automatically inherit its `major` bumps. This means that you do not need to
bump a crate that had a SemVer breaking change only from re-exporting another crate with a breaking change.  
`minor` an `patch` bumps do not need to be inherited, since `cargo` will automatically update them to the latest
compatible version.
