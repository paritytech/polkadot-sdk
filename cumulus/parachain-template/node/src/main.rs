//! Substrate Parachain Node Template CLI

#![warn(missing_docs)]

mod chain_spec;
mod service;
mod cli;
mod command;
mod rpc;

fn main() -> sc_cli::Result<()> {
	command::run()
}
