//! # Your first Runtime
//!
//! This guide will walk you through the steps needed to add your pallet to an existing runtime.
//!
//! The good news is, in [`crate::guides::your_first_pallet`], we have already created a _test_
//! runtime that was used for testing, and a real runtime is not that much different!
//!
//! ## Setup
//!
//! A runtime shares a few similar setup requirements as with a pallet:
//!
//! * importing [`frame`], [`parity_scale_codec`], and [`scale_info`] crates.
//! * following the [`std`](crate::polkadot_sdk::substrate#wasm-build) pattern.
//!
//! But, more specifically, it also contains:
//!
//! * a `build.rs` that uses [`substrate_wasm_builder`]. This entails declaring
//!   `[build-dependencies]` in the Cargo manifest file:
//!
//! ```
//! [build-dependencies]
//! substrate-wasm-builder = { ... }
//! ```
//! * a runtime must always be one-runtime-per-crate.
//!
//! You can find the full code of this guide in
//! [`polkadot_sdk_docs_guides_packages_first_runtime`].
//!
//! ## Your First Runtime
//!
//! The first new property of a real runtime that it must define its
//! [`frame::runtime::prelude::RuntimeVersion`]:
#![doc = docify::embed!("./packages/guides/first-runtime/src/lib.rs", VERSION)]
//!
//! The version contains a number of very important fields, such as `spec_version` and `spec_name`
//! that play an important role in identifying your runtime and its version. More about runtime
//! upgrades in [`crate::reference_docs::frame_runtime_upgrades_and_migrations`].
// TODO: explain spec better in its own doc and the ref doc on migration.
//!
//! Then, A real runtime also contains the `impl` of all individual pallets' `trait Config` for
//! `struct Runtime`, and a [`frame::runtime::prelude::construct_runtime`] macro that amalgamates
//! them all.
//!
//! In the case of our example:
#![doc = docify::embed!("./packages/guides/first-runtime/src/lib.rs", our_config_impl)]
//!
//! In this example, we bring in a number of other pallets from [`frame`] into the runtime:
#![doc = docify::embed!("./packages/guides/first-runtime/src/lib.rs", config_impls)]
//!
//! Notice how we use [`frame::pallet_macros::derive_impl`] to provide "default" configuration items
//! for each pallet. Feel free to dive into the definition of each default prelude (eg.
//! [`frame::prelude::frame_system::pallet::config_preludes::SolochainDefaultConfig`]) to learn more
//! which types are exactly used.
//!
//! > Recall that in [`crate::guides::your_first_pallet`], we provided `type AccountId = u64` to
//! > `frame_system`, while in this case we rely on whatever is provided by
//! > `SolochainDefaultConfig`, which is indeed a "real" 32 byte account id.
//!
//! Then, a familiar instance of `construct_runtime` amalgamates all of the pallets (and crucially,
//! defines )
#![doc = docify::embed!("./packages/guides/first-runtime/src/lib.rs", cr)]
//!
//! Recall from [`crate::reference_docs::wasm_meta_protocol`] that every (real) runtime needs to
//! implement a set of runtime APIs that will then let the node to communicate with it. The final
//! steps of crafting a runtime are related to achieving exactly this.
//!
//! First, we define a number of types that eventually lead to the creation of an instance of
//! [`frame::runtime::prelude::Executive`]. The executive is a handy FRAME utility that, through
//! amalgamating all pallets and further types, implements some of the very very core pieces of the
//! runtime logic, such as how blocks are executed and other runtime-api implementations.
#![doc = docify::embed!("./packages/guides/first-runtime/src/lib.rs", runtime_types)]
//!
//! Finally, we use [`frame::runtime::prelude::impl_runtime_apis`] to implement all of the runtime
//! APIs that the runtime wishes to expose. As you will see in the code, most of these runtime API
//! implementations are merely forwarding calls to `RuntimeExecutive` which handles the actual
//! logic. Given that the implementation block is somewhat large, we won't repeat it here. You can
//! look for `impl_runtime_apis!` in [`polkadot_sdk_docs_guides_packages_first_runtime`].
//!
//! ```
//! impl_runtime_apis! {
//! 	impl apis::Core<Block> for Runtime {
//! 		fn version() -> RuntimeVersion {
//! 			VERSION
//! 		}
//!
//! 		fn execute_block(block: Block) {
//! 			RuntimeExecutive::execute_block(block)
//! 		}
//!
//! 		fn initialize_block(header: &Header) -> ExtrinsicInclusionMode {
//! 			RuntimeExecutive::initialize_block(header)
//! 		}
//! 	}
//!
//! 	// many more trait impls...
//! }
//! ```
//!
//! And that more or less covers the details of how you would write a real, production ready
//! runtime!
//!
//! Once you compile a crate that contains a runtime as above, simply running `cargo build` will
//! generate the wasm blobs and place them under `./target/wbuild`, as explained
//! [here](crate::polkadot_sdk::substrate#wasm-build).
//!
//! ## Further Reading
//!
//! 1. To learn more about signed extensions, see [`crate::reference_docs::signed_extensions`].
//! 2. `AllPalletsWithSystem` is also generated by `construct_runtime`, as explained in
//!    [`crate::reference_docs::frame_runtime_types`].
//! 3. `Executive` supports more generics, most notably allowing the runtime to configure more
//!    runtime migrations, as explained in
//!    [`crate::reference_docs::frame_runtime_upgrades_and_migrations`].
//! 4. Learn more about adding and implementing runtime apis in
//!    [`crate::reference_docs::frame_rpc_runtime_apis`].
//!
//! ## Next Step
//!
//! See [`crate::guides::your_first_node`].
