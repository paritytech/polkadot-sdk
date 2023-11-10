// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! This crate includes structs and utilities for defining configuration files (known as chain
//! specification) for both runtime and node.
//!
//! # Intro: Chain Specification
//!
//! The chain specification comprises parameters and settings that define the properties and an
//! initial state of a chain. Users typically interact with the JSON representation of the chain
//! spec. Internally, the chain spec is embodied by the [`GenericChainSpec`] struct, and specific
//! properties can be accessed using the [`ChainSpec`] trait.
//!
//! In summary, although not restricted to, the primary role of the chain spec is to provide a list
//! of well-known boot nodes for the blockchain network and the means for initializing the genesis
//! storage. This initialization is necessary for creating a genesis block upon which subsequent
//! blocks are built. When the node is launched for the first time, it reads the chain spec,
//! initializes the genesis block, and establishes connections with the boot nodes.
//!
//! The JSON chain spec is divided into two main logical sections:
//! - one section details general chain properties,
//! - second explicitly or indirectly defines the genesis storage, which, in turn, determines the
//!   genesis hash of the chain,
//!
//! The chain specification consists of the following fields:
//!
//! <table>
//!   <thead>
//!     <tr>
//!       <th>Chain spec key</th>
//!       <th>Description</th>
//!     </tr>
//!   </thead>
//!   <tbody>
//!     <tr>
//!       <td>name</td>
//!       <td>The human readable name of the chain.</td>
//!     </tr>
//!     <tr>
//!       <td>id</td>
//!       <td>The id of the chain.</td>
//!     </tr>
//!     <tr>
//!       <td>chainType</td>
//!       <td>The chain type of this chain
//!           (refer to
//!            <a href="enum.ChainType.html" title="enum sc_chain_spec::ChainType">
//!              <code>ChainType</code>
//!            </a>).
//!       </td>
//!     </tr>
//!     <tr>
//!       <td>bootNodes</td>
//!       <td>A list of
//!       <a href="https://github.com/multiformats/multiaddr">multi addresses</a>
//!       that belong to boot nodes of the chain.</td>
//!     </tr>
//!     <tr>
//!       <td>telemetryEndpoints</td>
//!       <td>Optional list of <code>multi address, verbosity</code> of telemetry endpoints. The
//! verbosity goes from 0 to 9. With 0 being the mode with the lowest verbosity.</td>
//!     </tr>
//!     <tr>
//!       <td>protocolId</td>
//!       <td>Optional networking protocol id that identifies the chain.</td>
//!     </tr>
//!     <tr>
//!       <td>forkId</td>
//!       <td>Optional fork id. Should most likely be left empty. Can be used to signal a fork on
//! the network level when two chains have the same genesis hash.</td>
//!     </tr>
//!     <tr>
//!       <td>properties</td>
//!       <td>Custom properties. Shall be provided in the form of
//! <code>key</code>-<code>value</code> json object.
//!     </td>
//!     </tr>
//!     <tr>
//!       <td>consensusEngine</td>
//!       <td>Deprecated field. Should be ignored.</td>
//!     </tr>
//!     <tr>
//!       <td>codeSubstitutes</td>
//!       <td>Optional map of <code>block_number</code> to <code>wasm_code</code>. More details in
//! material to follow.</td>
//!     </tr>
//!     <tr>
//!       <td>genesis</td>
//!       <td>Defines the initial state of the runtime. More details in material to follow.</td>
//!     </tr>
//!   </tbody>
//! </table>
//!
//! # `genesis`: Initial Runtime State
//!
//! All nodes in the network must build subsequent blocks upon exactly the same genesis block.
//!
//! The information configured in the `genesis` section of a chain specification is used to build
//! the genesis storage, which is essential for creating the genesis block, since the block header
//! includes the storage root hash.
//!
//! The `genesis` key of the chain specification definition describes the
//! initial state of the runtime. For example, it may contain:
//! - an initial list of funded accounts,
//! - the administrative account that controls the sudo key,
//! - an initial authorities set for consensus, etc.
//!
//! As the compiled WASM blob of the runtime code is stored in the chain's state, the initial
//! runtime must also be provided within the chain specification.
//!
//! In essence, the most important formats of genesis initial state are:
//!
//! <table>
//!   <thead>
//!     <tr>
//!       <th>Format</th>
//!       <th>Description</th>
//!     </tr>
//!   </thead>
//!   <tbody>
//!     <tr>
//!       <td>
//! 		<code>runtime / full config</code>
//!       </td>
//!       <td>A JSON object that provides an explicit and comprehensive representation of the
//! <code>RuntimeGenesisConfig</code> struct, which is generated by <a
//! href="../frame_support_procedural/macro.construct_runtime.html"
//! ><code>frame::runtime::prelude::construct_runtime</code></a> macro (<a
//! href="../substrate_test_runtime/struct.RuntimeGenesisConfig.html#"
//! >example of generated struct</a>). Must contain all the keys of
//! the genesis config, no defaults will be used.
//!
//! This format explicitly provides the code of the runtime.
//! </td></tr>
//!     <tr>
//!       <td>
//! 		<code>patch</code>
//!       </td>
//!       <td>A JSON object that offers a partial representation of the
//!       <code>RuntimeGenesisConfig</code> provided by the runtime. It contains a patch, which is
//! essentially a list of key-value pairs to customize in the default runtime's
//! <code>RuntimeGenesisConfig</code>.
//! This format explicitly provides the code of the runtime.
//! </td></tr>
//!     <tr>
//!       <td>
//! 		<code>raw</code>
//!       </td>
//!       <td>A JSON object with two fields: <code>top</code> and <code>children_default</code>.
//! Each field is a map of <code>key => value</code> pairs representing entries in a genesis storage
//! trie. The runtime code is one of such entries.</td>
//!     </tr>
//!   </tbody>
//! </table>
//!
//! For production or long-lasting blockchains, using the `raw` format in the chain specification is
//! recommended. Only the `raw` format guarantees that storage root hash will remain unchanged when
//! the `RuntimeGenesisConfig` format changes due to software upgrade.
//!
//! JSON examples in the [following section](#json-chain-specification-example) illustrate the `raw`
//! `patch` and full genesis fields.
//!
//! # From Initial State to Raw Genesis.
//!
//! To generate a raw genesis storage from the JSON representation of the runtime genesis config,
//! the node needs to interact with the runtime.
//!
//! This interaction involves passing the runtime genesis config JSON blob to the runtime using the
//! [`sp_genesis_builder::GenesisBuilder::build_config`] function. During this operation, the
//! runtime converts the JSON representation of the genesis config into [`sp_io::storage`] items. It
//! is a crucial step for computing the storage root hash, which is a key component in determining
//! the genesis hash.
//!
//! Consequently, the runtime must support the [`sp_genesis_builder::GenesisBuilder`] API to
//! utilize either `patch` or `full` formats.
//!
//! This entire process is encapsulated within the implementation of the [`BuildStorage`] trait,
//! which can be accessed through the [`ChainSpec::as_storage_builder`] method. There is an
//! intermediate internal helper that facilitates this interaction,
//! [`GenesisConfigBuilderRuntimeCaller`], which serves as a straightforward wrapper for
//! [`sc_executor::WasmExecutor`].
//!
//! In case of `raw` genesis state the node does not interact with the runtime regarding the
//! computation of initial state.
//!
//! The plain and `raw` chain specification JSON blobs can be found in
//! [JSON examples](#json-chain-specification-example) section.
//!
//! # Optional Code Mapping
//!
//! Optional map of `block_number` to `wasm_code`.
//!
//! The given `wasm_code` will be used to substitute the on-chain wasm code starting with the
//! given block number until the `spec_version` on-chain changes. The given `wasm_code` should
//! be as close as possible to the on-chain wasm code. A substitute should be used to fix a bug
//! that cannot be fixed with a runtime upgrade, if for example the runtime is constantly
//! panicking. Introducing new runtime APIs isn't supported, because the node
//! will read the runtime version from the on-chain wasm code.
//!
//! Use this functionality only when there is no other way around it, and only patch the problematic
//! bug; the rest should be done with an on-chain runtime upgrade.
//!
//! # Building a Chain Specification
//!
//! The [`ChainSpecBuilder`] should be used to create an instance of a chain specification. Its API
//! allows configuration of all fields of the chain spec. To generate a JSON representation of the
//! specification, use [`ChainSpec::as_json`].
//!
//! The sample code to generate a chain spec is as follows:
#![doc = docify::embed!("src/chain_spec.rs", build_chain_spec_with_patch_works)]
//! # JSON chain specification example
//!
//! The following are the plain and `raw` versions of the chain specification JSON files, resulting
//! from executing of the above [example](#building-a-chain-specification):
//! ```ignore
#![doc = include_str!("../res/substrate_test_runtime_from_patch.json")]
//! ```
//! ```ignore
#![doc = include_str!("../res/substrate_test_runtime_from_patch_raw.json")]
//! ```
//! The following example shows the plain full config version of chain spec:
//! ```ignore
#![doc = include_str!("../res/substrate_test_runtime_from_config.json")]
//! ```
//! The [`ChainSpec`] trait represents the API to access values defined in the JSON chain specification.
//!
//!
//! # Custom Chain Spec Extensions
//!
//! The basic chain spec type containing all required parameters is [`GenericChainSpec`]. It can be
//! extended with additional options containing configuration specific to your chain. Usually, the
//! extension will be a combination of types exposed by Substrate core modules.
//!
//! To allow the core modules to retrieve their configuration from your extension, you should use
//! `ChainSpecExtension` macro exposed by this crate.
//! ```rust
//! use std::collections::HashMap;
//! use sc_chain_spec::{GenericChainSpec, ChainSpecExtension};
//!
//! #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ChainSpecExtension)]
//! pub struct MyExtension {
//! 	pub known_blocks: HashMap<u64, String>,
//! }
//!
//! pub type MyChainSpec<G> = GenericChainSpec<G, MyExtension>;
//! ```
//! Some parameters may require different values depending on the current blockchain height (a.k.a.
//! forks). You can use the [`ChainSpecGroup`](macro@ChainSpecGroup) macro and the provided [`Forks`]
//! structure to add such parameters to your chain spec. This will allow overriding a single
//! parameter starting at a specific block number.
//! ```rust
//! use sc_chain_spec::{Forks, ChainSpecGroup, ChainSpecExtension, GenericChainSpec};
//!
//! #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ChainSpecGroup)]
//! pub struct ClientParams {
//! 	max_block_size: usize,
//! 	max_extrinsic_size: usize,
//! }
//!
//! #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ChainSpecGroup)]
//! pub struct PoolParams {
//! 	max_transaction_size: usize,
//! }
//!
//! #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ChainSpecGroup, ChainSpecExtension)]
//! pub struct Extension {
//! 	pub client: ClientParams,
//! 	pub pool: PoolParams,
//! }
//!
//! pub type BlockNumber = u64;
//!
//! /// A chain spec supporting forkable `ClientParams`.
//! pub type MyChainSpec1<G> = GenericChainSpec<G, Forks<BlockNumber, ClientParams>>;
//!
//! /// A chain spec supporting forkable `Extension`.
//! pub type MyChainSpec2<G> = GenericChainSpec<G, Forks<BlockNumber, Extension>>;
//! ```
//! It's also possible to have a set of parameters that are allowed to change with block numbers
//! (i.e., they are forkable), and another set that is not subject to changes. This can also be
//! achieved by declaring an extension that contains [`Forks`] within it.
//! ```rust
//! use serde::{Serialize, Deserialize};
//! use sc_chain_spec::{Forks, GenericChainSpec, ChainSpecGroup, ChainSpecExtension};
//!
//! #[derive(Clone, Debug, Serialize, Deserialize, ChainSpecGroup)]
//! pub struct ClientParams {
//! 	max_block_size: usize,
//! 	max_extrinsic_size: usize,
//! }
//!
//! #[derive(Clone, Debug, Serialize, Deserialize, ChainSpecGroup)]
//! pub struct PoolParams {
//! 	max_transaction_size: usize,
//! }
//!
//! #[derive(Clone, Debug, Serialize, Deserialize, ChainSpecExtension)]
//! pub struct Extension {
//! 	pub client: ClientParams,
//! 	#[forks]
//! 	pub pool: Forks<u64, PoolParams>,
//! }
//!
//! pub type MyChainSpec<G> = GenericChainSpec<G, Extension>;
//! ```
//! The chain spec can be extended with other fields that are opaque to the default chain spec.
//! Specific node implementations will need to be able to deserialize these extensions.

mod chain_spec;
mod extension;
mod genesis_block;
mod genesis_config_builder;
mod json_patch;

pub use self::{
	chain_spec::{
		update_code_in_json_chain_spec, ChainSpec as GenericChainSpec, ChainSpecBuilder,
		NoExtension,
	},
	extension::{get_extension, get_extension_mut, Extension, Fork, Forks, GetExtension, Group},
	genesis_block::{
		construct_genesis_block, resolve_state_version_from_wasm, BuildGenesisBlock,
		GenesisBlockBuilder,
	},
	genesis_config_builder::GenesisConfigBuilderRuntimeCaller,
};
pub use sc_chain_spec_derive::{ChainSpecExtension, ChainSpecGroup};

use sc_network::config::MultiaddrWithPeerId;
use sc_telemetry::TelemetryEndpoints;
use serde::{de::DeserializeOwned, Serialize};
use sp_core::storage::Storage;
use sp_runtime::BuildStorage;

/// The type of a chain.
///
/// This can be used by tools to determine the type of a chain for displaying
/// additional information or enabling additional features.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Clone)]
pub enum ChainType {
	/// A development chain that runs mainly on one node.
	Development,
	/// A local chain that runs locally on multiple nodes for testing purposes.
	Local,
	/// A live chain.
	Live,
	/// Some custom chain type.
	Custom(String),
}

impl Default for ChainType {
	fn default() -> Self {
		Self::Live
	}
}

/// Arbitrary properties defined in chain spec as a JSON object
pub type Properties = serde_json::map::Map<String, serde_json::Value>;

/// A set of traits for the runtime genesis config.
pub trait RuntimeGenesis: Serialize + DeserializeOwned + BuildStorage {}
impl<T: Serialize + DeserializeOwned + BuildStorage> RuntimeGenesis for T {}

/// Common interface of a chain specification.
pub trait ChainSpec: BuildStorage + Send + Sync {
	/// Spec name.
	fn name(&self) -> &str;
	/// Spec id.
	fn id(&self) -> &str;
	/// Type of the chain.
	fn chain_type(&self) -> ChainType;
	/// A list of bootnode addresses.
	fn boot_nodes(&self) -> &[MultiaddrWithPeerId];
	/// Telemetry endpoints (if any)
	fn telemetry_endpoints(&self) -> &Option<TelemetryEndpoints>;
	/// Network protocol id.
	fn protocol_id(&self) -> Option<&str>;
	/// Optional network fork identifier. `None` by default.
	fn fork_id(&self) -> Option<&str>;
	/// Additional loosly-typed properties of the chain.
	///
	/// Returns an empty JSON object if 'properties' not defined in config
	fn properties(&self) -> Properties;
	/// Returns a reference to the defined chain spec extensions.
	fn extensions(&self) -> &dyn GetExtension;
	/// Returns a mutable reference to the defined chain spec extensions.
	fn extensions_mut(&mut self) -> &mut dyn GetExtension;
	/// Add a bootnode to the list.
	fn add_boot_node(&mut self, addr: MultiaddrWithPeerId);
	/// Return spec as JSON.
	fn as_json(&self, raw: bool) -> Result<String, String>;
	/// Return StorageBuilder for this spec.
	fn as_storage_builder(&self) -> &dyn BuildStorage;
	/// Returns a cloned `Box<dyn ChainSpec>`.
	fn cloned_box(&self) -> Box<dyn ChainSpec>;
	/// Set the storage that should be used by this chain spec.
	///
	/// This will be used as storage at genesis.
	fn set_storage(&mut self, storage: Storage);
	/// Returns code substitutes that should be used for the on chain wasm.
	fn code_substitutes(&self) -> std::collections::BTreeMap<String, Vec<u8>>;
}

impl std::fmt::Debug for dyn ChainSpec {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(f, "ChainSpec(name = {:?}, id = {:?})", self.name(), self.id())
	}
}
