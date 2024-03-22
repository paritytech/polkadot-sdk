//! Substrate Node Template CLI library.
#![warn(missing_docs)]

mod cli;
mod command;
mod rpc;
mod service;
mod standards;

fn main() -> sc_cli::Result<()> {
	command::run()
}
