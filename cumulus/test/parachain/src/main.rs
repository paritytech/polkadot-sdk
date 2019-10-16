//! Substrate Node Template CLI library.

#![warn(missing_docs)]
#![warn(unused_extern_crates)]

mod chain_spec;
#[macro_use]
mod service;
mod cli;

pub use substrate_cli::{VersionInfo, IntoExit, error};

fn main() -> Result<(), cli::error::Error> {
	let version = VersionInfo {
		name: "Cumulus Test Parachain Collator",
		commit: env!("VERGEN_SHA_SHORT"),
		version: env!("CARGO_PKG_VERSION"),
		executable_name: "cumulus-test-parachain-collator",
		author: "Parity Technologies <admin@parity.io>",
		description: "Cumulus test parachain collator",
		support_url: "https://github.com/paritytech/cumulus/issues/new",
	};

	cli::run(std::env::args(), cli::Exit, version)
}
