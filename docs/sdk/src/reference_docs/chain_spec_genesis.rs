//! # What is a chain specification
//!
//! - network / logical properties of the chain, the most important property being the list of
//!
//!
//!
//!   - raw
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/src/pallets.rs", pallet_bar_GenesisConfig)]
//!
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/src/pallets.rs", pallet_bar)]
//!
//! block:
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/src/pallets.rs", pallet_bar_build)]
//!
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/src/pallets.rs", pallet_foo_GenesisConfig)]
//!
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/src/pallets.rs", SomeFooData2)]
//!
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/src/pallets.rs", pallet_foo_build)]
//!
//!
//! be sneak-peeked here: [`RuntimeGenesisConfig`]. For further reading on generated runtime
//!
//!
//!
//!
//!
//!
//! or as a built-in runtime preset. More info on presets is in the material to follow.
//!
//!
//! [`build_state`], [`get_preset`].
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/src/runtime.rs", runtime_impl)]
//!
//! [`chain_spec_guide_runtime::presets::get_builtin_preset`]
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/src/presets.rs", get_builtin_preset)]
//!
//!
//! others useful for testing.
//!
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/src/presets.rs", preset_2)]
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/src/presets.rs", preset_3)]
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/src/presets.rs", preset_1)]
//!
//! simplify maintenance of built-in presets. The following example illustrates a runtime genesis
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/src/presets.rs", preset_4)]
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/tests/chain_spec_builder_tests.rs", preset_4_json)]
//!
//!
//!
//! that can be taken for testing:
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/src/presets.rs", check_presets)]
//!
//!
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/src/presets.rs", preset_invalid)]
//! `GenesisConfig`.
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/src/presets.rs", invalid_preset_works)]
//!
//!
//!
//! evolving over time. The JSON representation created at some point in time may no longer be
//!
//! [_chain-spec-examples_][`sc_chain_spec`].
//!
//!
//! the following command:
//!
//!
//!
//! ```
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/tests/chain_spec_builder_tests.rs", list_presets)]
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/tests/chain_spec_builder_tests.rs", get_preset)]
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/tests/chain_spec_builder_tests.rs", generate_chain_spec)]
#![doc = docify::embed!("./src/reference_docs/chain_spec_runtime/tests/chain_spec_builder_tests.rs", generate_para_chain_spec)]
//!
//!     chain_spec_guide_runtime::pallets::FooStruct
//! [`genesis_build`]: frame_support::pallet_macros::genesis_build
//! [`serde`]: https://serde.rs/field-attrs.html
//! [`camelCase`]: https://serde.rs/container-attrs.html#rename_all

// [`frame_runtime_types`]: frame_runtime_types
// [`sc_chain_spec`]: sc_chain_spec#json-chain-specification-example
