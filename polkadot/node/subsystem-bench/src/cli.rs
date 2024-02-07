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
use super::availability::DataAvailabilityReadOptions;
use crate::approval::ApprovalsOptions;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, clap::Parser)]
#[clap(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub struct TestSequenceOptions {
	#[clap(short, long, ignore_case = true)]
	pub path: String,
}

/// Supported test objectives
#[derive(Debug, Clone, clap::Parser, Serialize, Deserialize)]
#[command(rename_all = "kebab-case")]
pub enum TestObjective {
	/// Benchmark availability recovery strategies.
	DataAvailabilityRead(DataAvailabilityReadOptions),
	/// Benchmark availability and bitfield distribution.
	DataAvailabilityWrite,
	/// Run a test sequence specified in a file
	TestSequence(TestSequenceOptions),
	/// Benchmark the approval-voting and approval-distribution subsystems.
	ApprovalVoting(ApprovalsOptions),
	Unimplemented,
}

impl std::fmt::Display for TestObjective {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"{}",
			match self {
				Self::DataAvailabilityRead(_) => "DataAvailabilityRead",
				Self::DataAvailabilityWrite => "DataAvailabilityWrite",
				Self::TestSequence(_) => "TestSequence",
				Self::ApprovalVoting(_) => "ApprovalVoting",
				Self::Unimplemented => "Unimplemented",
			}
		)
	}
}

#[derive(Debug, clap::Parser)]
#[clap(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub struct StandardTestOptions {
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

	#[clap(short, long, ignore_case = true, default_value_t = 1)]
	/// The number of blocks the test is going to run.
	pub num_blocks: usize,
}
