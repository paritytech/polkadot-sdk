//! Substrate Parachain Node Template CLI

#![warn(missing_docs)]

mod chain_spec;
mod cli;
mod command;
mod rpc;
mod service;

fn main() {
	if let Err(error) = command::run() {
		eprintln!("Error: {error}");
		std::process::exit(1);
	}
}
