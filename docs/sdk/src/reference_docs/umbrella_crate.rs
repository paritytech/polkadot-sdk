//! # Umbrella Crate
//!
//! The Polkadot-SDK "umbrella" is a crate that re-exports all other published crates. This makes it
//! possible to have a very small `Cargo.toml` file that only has one dependency, the umbrella
//! crate. This helps with selecting the right combination of crate versions, since otherwise 3rd
//! party tools are needed to select a compatible set of versions.
//!
//!
//! ## Features
//!
//! The umbrella crate supports no-std builds and can therefore be used in the runtime and node.
//! There are two main features: `runtime` and `node`. The `runtime` feature enables all `no-std`
//! crates, while the `node` feature enables all `std` crates. It should be used like any other
//! crate in the repo, with `default-features = false`.
//!
//! For more fine-grained control, additionally, each crate can be enabled selectively. The umbrella
//! exposes one feature per dependency. For example, if you only want to use the `frame-support`
//! crate, you can enable the `frame-support` feature.
//!
//! The umbrella exposes a few more general features:
//! - `tuples-96`: Needs to be enabled for runtimes that have more than 64 pallets.
//! - `serde`: Specifically enable `serde` en/decoding support.
//! - `experimental`: Experimental enable experimental features - should not yet used in production.
//! - `with-tracing`: Enable tracing support.
//! - `try-runtime`, `runtime-benchmarks` and `std`: These follow the standard conventions.
//! - `runtime`: As described above, enable all `no-std` crates.
//! - `node`: As described above, enable all `std` crates.
//! - There does *not* exist a dedicated docs feature. To generate docs, enable the `runtime` and
//!   `node` feature. For `docs.rs` the manifest contains specific configuration to make it show up
//!   all re-exports.
//!
//! There is a specific [`zepter`](https://github.com/ggwpez/zepter) check in place to ensure that
//! the features of the umbrella are correctly configured. This check is run in CI and locally when
//! running `zepter`.
//!
//! ## Generation
//!
//! The umbrella crate needs to be updated every time when a new crate is added or removed from the
//! workspace. It is checked in CI by calling its generation script. The generation script is
//! located in `./scripts/generate-umbrella.py` and needs dependency `cargo_workspace`.
//!
//! Example: `python3 scripts/generate-umbrella.py --sdk . --version 1.9.0`
//!
//! ## Usage
//!
//! > Note: You can see a live example in the `staging-node-cli` and `kitchensink-runtime` crates.
//!
//! The umbrella crate can be added to your runtime crate like this:
//!
//! `polkadot-sdk = { path = "../../../../umbrella", features = ["runtime"], default-features =
//! false }`
//!
//! or for a node:
//!
//! `polkadot-sdk = { path = "../../../../umbrella", features = ["node"], default-features = false
//! }`
//!
//! In the code, it is then possible to bring all dependencies into scope via:
//!
//! `use polkadot_sdk::*;`
//!
//! ### Known Issues
//!
//! The only known issue so far is the fact that the `use` statement brings the dependencies only
//! into the outer module scope - not the global crate scope. For example, the following code would
//! need to be adjusted:
//!
//! ```rust
//! use polkadot_sdk::*;
//!
//! mod foo {
//!    // This does sadly not compile:
//!    frame_support::parameter_types! { }
//!
//!    // Instead, we need to do this (or add an equivalent `use` statement):
//!    polkadot_sdk::frame_support::parameter_types! { }
//! }
//! ```
//!
//! Apart from this, no issues are known. There could be some bugs with how macros locate their own
//! re-exports. Please [report issues](https://github.com/paritytech/polkadot-sdk/issues) that arise from using this crate.
//!
//! ## Dependencies
//!
//! The umbrella crate re-exports all published crates, with a few exceptions:
//! - Runtime crates like `rococo-runtime` etc are not exported. This otherwise leads to very weird
//!   compile errors and should not be needed anyway.
//! - Example and fuzzing crates are not exported. This is currently detected by checking the name
//!   of the crate for these magic words. In the future, it will utilize custom metadata, as it is
//!   done in the `rococo-runtime` crate.
//! - The umbrella crate itself. Should be obvious :)
