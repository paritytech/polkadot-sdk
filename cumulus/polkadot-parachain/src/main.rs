// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Polkadot parachain node.

#![warn(missing_docs)]
#![warn(unused_extern_crates)]

mod chain_spec;

use polkadot_parachain_lib::{run, CommandConfig};

fn main() -> color_eyre::eyre::Result<()> {
	color_eyre::install()?;

	let config = CommandConfig {
		chain_spec_loader: Some(Box::new(chain_spec::ChainSpecLoader)),
		runtime_resolver: Some(Box::new(chain_spec::RuntimeResolver)),
	};
	Ok(run(config)?)
}
