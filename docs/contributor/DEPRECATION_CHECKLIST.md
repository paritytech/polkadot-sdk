# Deprecation Checklist

Polkadot SDK is under constant development and improvement, thus deprecation and removal of existing code happen often.
When creating a breaking change we need to be mindful that external builders could be impacted by this.
The deprecation checklist tries to mitigate this impact, while still keeping the developer experience, the DevEx, as
smooth as possible.

To start a deprecation process, a new issue with the label `T15-deprecation` needs to be created for correct tracking.
Then these are the actions to take:

## Hard deprecate by adding a warning message

The warning message shall include a removal month and year, which is suggested to be 6 months after the deprecation
notice is released.
This means that the code will be removed in a release within that month (or after, but never before). Please use this
template, doing so makes it easy to search through the code base:

```rust
#[deprecated(note = "[DEPRECATED] will be removed after [DATE]. [ALTERNATIVE]")]
```
`[ALTERNATIVE]` won't always be possible but offer it if it is.

E.g.
```rust
#[deprecated(note = "`GenesisConfig` will be removed after December 2023. Use `RuntimeGenesisConfig` instead.")]
```

Some pieces of code cannot be labeled as deprecated, like [reexports](https://github.com/rust-lang/rust/issues/30827)
or [dispatchables](https://github.com/paritytech/polkadot-sdk/issues/182#issuecomment-1691684159), for instance.
In cases like that we can only make a visible enough comment, and make sure that we [announce the deprecation properly](#announce-the-deprecation-and-removal).

## Remove usage of the deprecated code in the code base

Just make sure that we are not using the deprecated code ourselves.
If you added the deprecation warning from the previous step, this can be done by making sure that warning is not shown
when building the code.

## Update examples and tutorials

Make sure that the rust docs are updated.
We also need [https://docs.substrate.io/](https://docs.substrate.io/) to be updated accordingly. The repo behind it is
[https://github.com/substrate-developer-hub/substrate-docs](https://github.com/substrate-developer-hub/substrate-docs).

## Announce the deprecation and removal

**At minimum they should be noted in the release log.** Please see how to document a PR [here](https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/CONTRIBUTING.md#documentation).
There you can give instructions based on the audience and tell them what they need to do to upgrade the code.

Some breaking changes have a bigger impact than others. When the impact is big the release note is not enough, though
it should still be the primary place for the notice. You can link back to the changelog files in other channels if you
want to announce it somewhere else.
Make sure you are as loud as you need to be for the magnitude of the breaking change.

## Removal version is planned

Depending on the removal date indicated in the deprecation warning in the [first step](#hard-deprecate-by-adding-a-warning-message),
the nature and the importance of the change, it might make sense to coordinate the release with other developers and
with the Release team.

## Deprecated code is removed

The deprecated code finally gets removed.
Don’t forget to [announce this accordingly](#announce-the-deprecation-and-removal).

✅ In order to not forget any of these steps, consider using this template in your deprecation issue:

```markdown
### Tasks

- [ ] Deprecate code by adding a warning message
- [ ] Remove usage of the deprecated code in the code base
- [ ] Update examples and tutorials
- [ ] Announce code deprecation
- [ ] Plan removal version
- [ ] Announce code removal
- [ ] Remove deprecated code
```
