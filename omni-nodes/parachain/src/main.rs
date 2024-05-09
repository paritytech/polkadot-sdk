//! Substrate Parachain Node Template CLI

#![warn(missing_docs)]

mod builder;
mod cli;
mod command;
mod rpc;
mod service;
mod standards;

use crate::builder::Builder;

// TODO: for statemint and statemine, inject a custon on_load that will rename some things.

fn main() -> sc_cli::Result<()> {
	let node = Builder::default().build()?;
	node.run()
}
