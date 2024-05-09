//! Substrate Parachain Node Template CLI

#![warn(missing_docs)]

mod builder;
mod cli;
mod command;
mod rpc;
mod service;
mod standards;

use crate::builder::Builder;

fn main() -> sc_cli::Result<()> {
	let node = Builder::default()
		// .parachain_consensus(builder::ParachainConsensus::Relay(12))
		.parachain_consensus(builder::ParachainConsensus::Aura(12))
		.build()?;
	node.run()
}
