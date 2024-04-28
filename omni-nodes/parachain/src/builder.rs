//! This file is in essence a code-level parameterization of the `service.rs` file.

use crate::command;

type RpcTraitObj = ();

/// Variants of the block authoring we can support.
pub enum Authoring {
	ManualSeal(Option<u32>),
	InstantSeal,
	Aura,
	Babe,
	PoW,
	FreeForAll,
}

pub enum Finality {
	/// No finality.
	None,
	/// Grandpa finality.
	Grandpa,
	/// Relay chain finality is our finality.
	RelayChain,
}

/// The types of node that can be produced. Some option might possibly not be compatible with
/// [`NodeType`]s.
pub enum NodeType {
	/// A node that is a solochain. Standard, bare-bone blockchain stuff, fueled by the
	/// polkadot-sdk crates.
	Solochain,
	/// A parachain node. This will run the node with all the cumulus related crates to make it a
	/// collator.
	Parachain,
}

/// Environment of the node. Helps heuristically prevent mistaken configurations, such as using
/// [`Authoring::InstantSeal`] in [`Env::Production`].
pub enum Env {
	Testing,
	Production,
}

pub struct Builder {
	node_type: NodeType,

	authoring: Authoring,
	finality: Finality,

	extra_rpcs: Vec<Box<RpcTraitObj>>,

	offchain_worker: bool,
	prometheus: bool,
	telemetry: bool,
}

impl Default for Builder {
	fn default() -> Self {
		Self {
			authoring: Authoring::ManualSeal(Some(1000)),
			finality: Finality::None,

			offchain_worker: false,

			prometheus: false,
			telemetry: false,

			node_type: NodeType::Solochain,
			extra_rpcs: Default::default(),
		}
	}
}

impl Builder {
	// TODO: for now this is both `build` and `run`. `
	pub fn build(self) -> sc_cli::Result<()> {
		self.validate();
		command::run(self)
	}

	pub fn node_type(mut self, node_type: NodeType) -> Self {
		self.node_type = node_type;
		self
	}

	pub fn authoring(mut self, authoring: Authoring) -> Self {
		self.authoring = authoring;
		self
	}

	pub fn finality(mut self, finality: Finality) -> Self {
		self.finality = finality;
		self
	}

	pub fn extra_rpc(mut self, rpc: Box<RpcTraitObj>) -> Self {
		self.extra_rpcs.push(rpc);
		self
	}

	fn validate(&self) -> Result<(), ()> {
		todo!("validate the current `self` to be sane. Not all parameters go well with one another")
	}
}
