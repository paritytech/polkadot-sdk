//! This file is in essence a code-level parameterization of the `service.rs` file.

use cumulus_client_cli::CollatorOptions;

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
	parachain_consensus: ParachainConsensus,
	on_load: Option<
		Box<dyn FnOnce(sc_service::Configuration, CollatorOptions) -> Result<(), sc_cli::Error>>,
	>,

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
			parachain_consensus: ParachainConsensus::Relay,
			on_load: None,

			prometheus: false,
			telemetry: false,

			node_type: NodeType::Solochain,
			extra_rpcs: Default::default(),
		}
	}
}

pub enum ParachainConsensus {
	Relay,
	Aura,
	RelayAndAura,
	FreeForAll,
}

impl Builder {
	// TODO: for now this is both `build` and `run`. `
	pub fn build_and_run(self) -> sc_cli::Result<()> {
		self.validate();
		command::run(self)
	}

	pub fn consensus(mut self, consensus: ParachainConsensus) -> Self {
		self.parachain_consensus = consensus;
		self
	}

	pub fn on_load(
		mut self,
		on_load: Box<
			dyn FnOnce(sc_service::Configuration, CollatorOptions) -> Result<(), sc_cli::Error>,
		>,
	) -> Self {
		self.on_load = Some(on_load);
		self
	}

	fn validate(&self) -> Result<(), ()> {
		Ok(())
	}
}
