//! # Your first Runtime
//!
//!
//!
//!
//!
//!
//!
//!
//! ```
//!
//!
//!
//!
#![doc = docify::embed!("./packages/guides/first-runtime/src/lib.rs", VERSION)]
//!
//! [`frame_runtime_upgrades_and_migrations`].
//!
//!
#![doc = docify::embed!("./packages/guides/first-runtime/src/lib.rs", our_config_impl)]
//!
#![doc = docify::embed!("./packages/guides/first-runtime/src/lib.rs", config_impls)]
//!
//! used.
//!
//!
#![doc = docify::embed!("./packages/guides/first-runtime/src/lib.rs", cr)]
//!
//!
//! runtime logic, such as how blocks are executed and other runtime-api implementations.
#![doc = docify::embed!("./packages/guides/first-runtime/src/lib.rs", runtime_types)]
//!
//! logic. Given that the implementation block is somewhat large, we won't repeat it here. You can
//!
//! 		fn version() -> RuntimeVersion {
//!
//!
//! 	}
//!
//!
//!
//!
//!
//! primary way to run a new chain.
//!
//!
//! 			fn build_state(config: Vec<u8>) -> GenesisBuilderResult {
//!
//!
//! 		}
//!
//!
//!
#![doc = docify::embed!("./packages/guides/first-runtime/src/lib.rs", preset_names)]
//!
//!
//!
#![doc = docify::embed!("./packages/guides/first-runtime/src/lib.rs", development_config_genesis)]
//!
//!
//!
//!
//!
//! 3. `Executive` supports more generics, most notably allowing the runtime to configure more
//!    [`custom_runtime_api_rpc`].
//!

// Link References

// Link References




// [`frame_runtime_upgrades_and_migrations`]: frame_runtime_upgrades_and_migrations

// [`frame_runtime_types`]: frame_runtime_types
// [`frame_runtime_upgrades_and_migrations`]: frame_runtime_upgrades_and_migrations
// [`std` feature-gating`]: crate::polkadot_sdk::substrate#wasm-build
