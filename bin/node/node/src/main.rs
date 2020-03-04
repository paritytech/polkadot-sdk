//! Substrate Node Template CLI library.
#![warn(missing_docs)]

mod chain_spec;
#[macro_use]
mod service;
mod cli;
mod command;

fn main() -> sc_cli::Result<()> {
	let version = sc_cli::VersionInfo {
		name: "Bridge Node",
		commit: env!("VERGEN_SHA_SHORT"),
		version: env!("CARGO_PKG_VERSION"),
		executable_name: "bridge-node",
		author: "Parity Technologies",
		description: "Bridge Node",
		support_url: "https://github.com/paritytech/parity-bridges-common/",
		copyright_start_year: 2017,
	};

	command::run(version)
}
