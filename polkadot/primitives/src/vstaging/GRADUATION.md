When a high enough Polkadot version has been released we "graduate" the staging APIs by moving **some of** the code out of `vstaging` into the `v{N}` folder next to `vstaging`.

For a ticket about graduating ParachainHost API version 13 and v9 primitives, e.g. [ticket](https://github.com/paritytech/polkadot-sdk/issues/9400), this is the steps to follow:
0. Rename [`v8`](polkadot/primitives/src/v8) -> `v9`, resolve issues (Rust Analyzer might have changed `app_crypto!(sr25519, super::PARACHAIN_KEY_TYPE_ID);` to `app_crypto!(sr25519, v9::PARACHAIN_KEY_TYPE_ID);` [here](https://github.com/paritytech/polkadot-sdk/blob/4cd07c56378291fddb9fceab3b508cf99034126a/polkadot/primitives/src/v8/mod.rs#L103) and in a few more places in that file. Change back to `super::`).
0. Bump version in [comment here](https://github.com/paritytech/polkadot-sdk/blob/4cd07c56378291fddb9fceab3b508cf99034126a/polkadot/primitives/src/lib.rs#L22) to 13
0. Move all relevant code from `vstaging` folder into `v9`, by relevant we mean code for version 13 and below. If `vstaging` contains any version 14 code, it should remain in `vstaging`. If there is a name collision, e.g. `struct Foo` existing in `v9` (prev. `v8`) call rename the existing one `LegacyFoo` or similar.

# V9
## TODO
### Revert
Revert `pub(super)` vis on `CandidateDescriptorV2`, `CandidateUMPSignals`