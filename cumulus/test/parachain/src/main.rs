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

pub use substrate_cli::{VersionInfo, IntoExit, error};

/// The parachain id of this parachain.
pub const PARA_ID: ParaId = ParaId::new(100);

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
