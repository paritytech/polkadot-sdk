//! This file is in essence a code-level parameterization of the `service.rs` file.

use crate::{rpc::DenyUnsafe, standards::OpaqueBlock as Block};
use cumulus_client_cli::CollatorOptions;
use omni_node_common::fake_runtime::RuntimeApi;
use std::sync::Arc;

use crate::{command, rpc};

pub enum SolochainConsensus {
	ManualSeal(Option<u32>),
	InstantSeal,
	Aura,
	Babe,
	PoW,
}

pub enum SolochainFinality {
	None,
	Grandpa,
}

pub struct SolochainConfig {
	pub consensus: SolochainConsensus,
	pub finality: SolochainFinality,
	pub shared: SharedConfig,
}

pub enum ParachainConsensus {
	Relay(u64),
	Aura(u64),
	AuraAsyncBacking(u64),
	RelayToAura(u64),
}

pub struct ParachainConfig {
	pub shared: SharedConfig,
	pub consensus: ParachainConsensus,
}

pub struct SharedConfig {
	pub rpc_extensions: Vec<RpcExtensionFn>,
}

pub enum NodeType {
	Solochain(SolochainConfig),
	Parachain(ParachainConfig),
}

pub type RpcExtensionFn = Box<
	dyn cumulus_service::BuildRpcExtension<
		crate::service::parachain_service::Block,
		crate::service::parachain_service::RuntimeApi,
		crate::service::parachain_service::HostFunctions,
	>,
>;

// TODO: for now it is all public, but then extend this with a nice "typed-builder" pattern
pub struct Builder {
	/// A hook injected into the service creation process.
	pub on_service_load: Option<OnServiceLoadObj>,
	pub node_type: NodeType,
}

impl Default for Builder {
	fn default() -> Self {
		Self {
			on_service_load: None,
			node_type: NodeType::Parachain(ParachainConfig {
				shared: SharedConfig { rpc_extensions: vec![] },
				consensus: ParachainConsensus::Aura(12_000),
			}),
		}
	}
}

pub type OnServiceLoadObj = Box<
	dyn FnOnce(&sc_service::Configuration, Option<CollatorOptions>) -> Result<(), sc_cli::Error>,
>;

impl Builder {
	// TODO: for now this is both `build` and `run`. `
	pub fn build(self) -> sc_cli::Result<Node> {
		self.validate()?;
		Ok(Node { builder: self })
	}

	pub fn on_service_load(mut self, on_load: OnServiceLoadObj) -> Self {
		self.on_service_load = Some(on_load);
		self
	}

	pub fn node_type(mut self, node_type: NodeType) -> Self {
		self.node_type = node_type;
		self
	}

	pub fn extend_rpc(mut self, extension: RpcExtensionFn) -> Self {
		match &mut self.node_type {
			NodeType::Parachain(config) => {
				config.shared.rpc_extensions.push(extension);
			},
			NodeType::Solochain(config) => {
				config.shared.rpc_extensions.push(extension);
			},
		}
		self
	}

	fn validate(&self) -> sc_cli::Result<()> {
		// TODO: anything that might need checking here.
		Ok(())
	}
}

pub struct Node {
	// TODO: this should be a subset of the `Builder` struct that we actually need, but for now
	// lazily we pass it all here :D
	pub builder: Builder,
}

impl Node {
	pub fn run(self) -> sc_cli::Result<()> {
		// TODO: similar to above, probably don't pass all of this in, but re-architect the `fn run`
		// a bit nicer, probably more like `struct RunParams`.
		command::run(self.builder)
	}
}
