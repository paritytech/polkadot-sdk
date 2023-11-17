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

#[derive(Debug, clap::Parser, Clone)]
#[clap(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub struct NetworkOptions {}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
#[value(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum NetworkEmulation {
	Ideal,
	Healthy,
	Degraded,
}

#[derive(Debug, clap::Parser)]
#[clap(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub struct DataAvailabilityReadOptions {
	#[clap(long, ignore_case = true, default_value_t = 100)]
	/// Number of cores to fetch availability for.
	pub n_cores: usize,

	#[clap(long, ignore_case = true, default_value_t = 500)]
	/// Number of validators to fetch chunks from.
	pub n_validators: usize,

	#[clap(long, ignore_case = true, default_value_t = 5120)]
	/// The minimum pov size in KiB
	pub min_pov_size: usize,

	#[clap(long, ignore_case = true, default_value_t = 5120)]
	/// The maximum pov size bytes
	pub max_pov_size: usize,

	#[clap(short, long, default_value_t = false)]
	/// Turbo boost AD Read by fetching from backers first. Tipically this is only faster if nodes
	/// have enough bandwidth.
	pub fetch_from_backers: bool,

	#[clap(short, long, ignore_case = true, default_value_t = 1)]
	/// Number of times to block fetching for each core.
	pub num_blocks: usize,
}
