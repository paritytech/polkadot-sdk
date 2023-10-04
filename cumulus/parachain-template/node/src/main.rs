//! Substrate Parachain Node Template CLI

#![warn(missing_docs)]

mod chain_spec;
#[macro_use]
mod service;
mod cli;
mod command;
mod rpc;

fn main() {
	if let Err(error) = command::run() {
		eprintln!("Error: {error}");
		std::process::exit(1);
	}
}
