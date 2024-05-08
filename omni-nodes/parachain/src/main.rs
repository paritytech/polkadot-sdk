//! Substrate Parachain Node Template CLI

#![warn(missing_docs)]

mod builder;
mod cli;
mod command;
mod rpc;
mod service;
mod standards;

use crate::builder::Builder;

/*
In general, we have 3 category of things that can be customized:

1. execution time things: they can be either passed in as a CLI arg, or in the builder. Example: consensus.
2. types. These things need to be known at compile time. So we have two options:
  * they must be hardcoded in standards.rs, or
  * they are given to the builder type as a trait with defaults.
3.
*/

// TODO: for statemint and statemine, inject a custon on_load that will rename some things.

fn main() -> sc_cli::Result<()> {
	let node = Builder::default().build()?;
	node.run()
}
