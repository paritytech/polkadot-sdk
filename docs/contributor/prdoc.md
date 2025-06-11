# PRDoc

A [prdoc](https://github.com/paritytech/prdoc) is like a changelog but for a Pull Request. We use
this approach to record changes on a crate level. This information is then processed by the release
team to apply the correct crate version bumps and to generate the CHANGELOG of the next release.

## Requirements

When creating a PR, the author needs to decide with the `R0-silent` label whether the PR has to
contain a prdoc. The `R0` label should only be placed for No-OP changes like correcting a typo in a
comment or CI stuff. If unsure, ping the [CODEOWNERS](../../.github/CODEOWNERS) for advice.

## Auto Generation

You can create a PrDoc by using the `/cmd prdoc` command (see args with `/cmd prdoc --help`) in a
comment on your PR.

Options:

- `audience` The audience of whom the changes may concern.
  - `runtime_dev`: Anyone building a runtime themselves. For example parachain teams, or people
    providing template runtimes. Also devs using pallets, FRAME etc directly. These are people who
    care about the protocol (WASM), not the meta-protocol (client).
  - `runtime_user`: Anyone using the runtime. Can be front-end devs reading the state, exchanges
    listening for events, libraries that have hard-coded pallet indices etc. Anything that would
    result in an observable change to the runtime behaviour must be marked with this.
  - `node_dev`: Those who build around the client side code. Alternative client builders, SMOLDOT,
  those who consume RPCs. These are people who are oblivious to the runtime changes. They only care
  about the meta-protocol, not the protocol itself.
  - `node_operator`: People who run the node. Think of validators, exchanges, indexer services, CI
    actions. Anything that modifies how the binary behaves (its arguments, default arguments, error
    messags, etc) must be marked with this.
- `bump:`: The default bump level for all crates. The PrDoc will likely need to be edited to reflect
  the actual changes after generation. More details in the section below.
  - `none`: There is no observable change. So to say: if someone were handed the old and the new
    version of our software, it would be impossible to figure out what version is which.
  - `patch`: Fixes that will never cause compilation errors if someone updates to this version. No
    functionality has been changed. Should be limited to fixing bugs or No-OP implementation
    changes.
  - `minor`: Additions that will never cause compilation errors if someone updates to this version.
    No functionality has been changed. Should be limited to adding new features.
  - `major`: Anything goes.
- `force: true|false`: Whether to overwrite any existing PrDoc file.

### Example

```bash
/cmd prdoc --audience runtime_dev --bump patch
```

## Local Generation

A `.prdoc` file is a YAML file with a defined structure (ie JSON Schema). Please follow these steps
to generate one:

1. Install the [`prdoc` CLI](https://github.com/paritytech/prdoc) by running `cargo install
   parity-prdoc`.
1. Open a Pull Request and get the PR number.
1. Generate the file with `prdoc generate <PR_NUMBER>`. The output filename will be printed.
1. Optional: Install the `prdoc/schema_user.json` schema in your editor, for example
   [VsCode](https://github.com/paritytech/prdoc?tab=readme-ov-file#schemas).
1. Edit your `.prdoc` file according to the [Audience](#pick-an-audience) and
   [SemVer](#record-semver-changes) sections.
1. Check your prdoc with `prdoc check -n <PR_NUMBER>`. This is optional since the CI will also check
   it.

> **Tip:** GitHub CLI and jq can be used to provide the number of your PR to generate the correct
> file:  
> `prdoc generate $(gh pr view --json number | jq '.number') -o prdoc`

## Record SemVer Changes

All published crates that got modified need to have an entry in the `crates` section of your
`PRDoc`. This entry tells the release team how to bump the crate version prior to the next release.
It is very important that this information is correct, otherwise it could break the code of
downstream teams.

The bump can either be `major`, `minor`, `patch` or `none`. The three first options are defined by
[rust-lang.org](https://doc.rust-lang.org/cargo/reference/semver.html), whereas `None` should be
picked if no other applies. The `None` option is equivalent to the `R0-silent` label, but on a crate
level. Experimental and private APIs are exempt from bumping and can be broken at any time. Please
read the [Crate Section](../RELEASE.md) of the RELEASE doc about them.

### Example

For example when you modified two crates and record the changes:

```yaml
crates:
  - name: frame-example
    bump: major
  - name: frame-example-pallet
    bump: minor
```

It means that downstream code using `frame-example-pallet` is still guaranteed to work as before,
while code using `frame-example` might break.

### Dependencies

A crate that depends on another crate will automatically inherit its `major` bumps. This means that
you do not need to bump a crate that had a SemVer breaking change only from re-exporting another
crate with a breaking change.  
`minor` an `patch` bumps do not need to be inherited, since `cargo` will automatically update them
to the latest compatible version.

### Overwrite CI Check

The `check-semver` CI check is doing sanity checks based on the provided `PRDoc` and the mentioned
crate version bumps. The tooling is not perfect and it may recommends incorrect bumps of the version.
The CI check can be forced to accept the provided version bump. This can be done like:

```yaml
crates:
  - name: frame-example
    bump: major
    validate: false
  - name: frame-example-pallet
    bump: minor
```

By putting `validate: false` for `frame-example`, the version bump is ignored by the tooling. For
`frame-example-pallet` the version bump is still validated by the CI check.

### Backporting PRs

When [backporting changes](../BACKPORT.md) to a stable release branch (e.g. `stable2503`), stricter versioning rules
apply to minimise risk for downstream users.

#### ✅ Allowed Bump Levels

Only the following `bump` levels are allowed by default:

- `none`: No observable change. No detectable difference between old and new versions.
- `patch`: Bug fixes or internal changes. Do not affect functionality or cause compilation errors.
- `minor`: Backward-compatible additions. Safe to adopt; adds features only, no behaviour changes.

Backport PRs with `major` bumps will fail CI.

#### 🚨 Overriding the CI Check

If a `major` bump is truly needed, you must:

1. Set `validate: false` in the `.prdoc`. See [Overwrite CI Check](#overwrite-ci-check).
2. Include a justification in the PR description explaining:
    - Why the bump is necessary.
    - Why it is safe for downstream users.
3. Notify a release engineer or senior reviewer for approval.

> Use this override sparingly, and only when you’re confident the change is safe and justified.

