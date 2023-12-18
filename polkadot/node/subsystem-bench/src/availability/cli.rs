// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use serde::{Deserialize, Serialize};

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
#[value(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum NetworkEmulation {
	Ideal,
	Healthy,
	Degraded,
}

#[derive(Debug, Clone, Serialize, Deserialize, clap::Parser)]
#[clap(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub struct DataAvailabilityReadOptions {
	#[clap(short, long, default_value_t = false)]
	/// Turbo boost AD Read by fetching the full availability datafrom backers first. Saves CPU as
	/// we don't need to re-construct from chunks. Tipically this is only faster if nodes have
	/// enough bandwidth.
	pub fetch_from_backers: bool,
}
