// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Cumulus test parachain collator

#![warn(missing_docs)]
#![warn(unused_extern_crates)]

use polkadot_primitives::parachain::Id as ParaId;

mod chain_spec;
#[macro_use]
mod service;
mod cli;
mod command;

/// The parachain id of this parachain.
pub const PARA_ID: ParaId = ParaId::new(100);
const EXECUTABLE_NAME: &'static str = "cumulus-test-parachain-collator";
const DESCRIPTION: &'static str =
	"Cumulus test parachain collator\n\nThe command-line arguments provided first will be \
	passed to the parachain node, while the arguments provided after -- will be passed \
	to the relaychain node.\n\n\
	cumulus-test-parachain-collator [parachain-args] -- [relaychain-args]";

fn main() -> sc_cli::Result<()> {
	let version = sc_cli::VersionInfo {
		name: "Cumulus Test Parachain Collator",
		commit: env!("VERGEN_SHA_SHORT"),
		version: env!("CARGO_PKG_VERSION"),
		author: "Parity Technologies <admin@parity.io>",
		description: DESCRIPTION,
		executable_name: EXECUTABLE_NAME,
		support_url: "https://github.com/paritytech/cumulus/issues/new",
		copyright_start_year: 2017,
	};

	command::run(version)
}
