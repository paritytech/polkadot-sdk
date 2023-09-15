# Deprecation Process

This deprecation process makes sense while we don’t [SemVer](https://semver.org/), after that this process will most likely change.

As deprecation and removal of existing features can happen on any release we need to be mindful that external builders can be impacted by the changes we make.

This process tries to mitigate this impact, while still keeping the devex as smooth as possible.

First of all we need to create a new issue with the label `I11-deprecation` and add it to the [Runtime / FRAME](https://github.com/orgs/paritytech/projects/40) project. This will make sure that the issue is added to the [Deprecation board](https://github.com/orgs/paritytech/projects/40/views/12) for correct tracking.

These are the actions to take:

### Hard deprecate by adding a warning message

The warning message should include a removal month and year, which is suggested to be 6 months from the deprecation notice is released. This means that the feature will be removed in a release within that month (or after, but never before). Something on these lines:

```rust
#[deprecated(note = "`GenesisConfig` is planned to be removed in December 2023. 
Use `RuntimeGenesisConfig` instead.")]

```

Some features cannot be label as deprecated, like [reexports](https://github.com/rust-lang/rust/issues/30827) or [dispatchables](https://github.com/paritytech/polkadot-sdk/issues/182#issuecomment-1691684159) for instance. On cases like that we can only make a visible enough comment, and make sure that we [announce the deprecation properly](#announce-the-deprecation-and-removal).

### Remove usage of the deprecated feature in the code base

Just make sure that we are not using the deprecated feature ourselves. If you added the deprecation warning from the previous step, this should be easy to get done.

### Update examples and tutorials

Make sure that the rust docs is updated.

We also want [https://docs.substrate.io/](https://docs.substrate.io/) to be updated, you can open an issue on [https://github.com/substrate-developer-hub/substrate-docs](https://github.com/substrate-developer-hub/substrate-docs).

### Announce the deprecation and removal

**At minimum they should be noted in the release log.**
Sometimes the release note is not enough. Make sure you are as loud as you need to be for the magnitude of the breaking change. Some breaking changes have a bigger impact than others.

### Removal version is planned

Depending on the removal date indicated in the deprecation warning in the [first step](#hard-deprecate-by-adding-a-warning-message), the nature and the importance of the change, it might make sense to coordinate the release with other developers and with the Release team.

### Deprecated feature is removed

The deprecated feature gets finally removed. Don’t forget to [announce this accordingly](#announce-the-deprecation-and-removal).

✅ In order to not forget any of these steps, consider using this template in your deprecation issue:
```
### Tasks

-   [ ] Deprecate feature by adding a warning message
-   [ ] Remove usage of the deprecated feature in the code base
-   [ ] Update examples and tutorials
-   [ ] Announce feature deprecation
-   [ ] Plan removal version
-   [ ] Announce feature removal
-   [ ] Remove deprecated feature
```
