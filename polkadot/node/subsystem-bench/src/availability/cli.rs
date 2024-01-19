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
use strum::Display;

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
#[value(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum NetworkEmulation {
	Ideal,
	Healthy,
	Degraded,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Display)]
#[value(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum Strategy {
	/// Regular random chunk recovery. This is also the fallback for the next strategies.
	Chunks,
	/// Recovery from systematic chunks. Much faster than regular chunk recovery becasue it avoid
	/// doing the reed-solomon reconstruction.
	Systematic,
	/// Fetch the full availability datafrom backers first. Saves CPU as we don't need to
	/// re-construct from chunks. Typically this is only faster if nodes have enough bandwidth.
	FullFromBackers,
}

#[derive(Debug, Clone, Serialize, Deserialize, clap::Parser)]
#[clap(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub struct DataAvailabilityReadOptions {
	#[clap(short, long, default_value_t = Strategy::Systematic)]
	pub strategy: Strategy,
}
