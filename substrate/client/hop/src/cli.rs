// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! HOP CLI parameters.
//!
//! ## Usage
//!
//! To integrate HOP into your Substrate node CLI, flatten these parameters:
//!
//! ```rust,ignore
//! use sc_hop::HopParams;
//!
//! #[derive(Debug, clap::Parser)]
//! pub struct Cli {
//!     // ... your other CLI fields ...
//!
//!     #[clap(flatten)]
//!     pub hop: HopParams,
//! }
//! ```

use crate::types::{DEFAULT_MAX_POOL_SIZE, DEFAULT_RETENTION_BLOCKS};
use clap::Parser;

/// HOP (Hand-Off Protocol) configuration parameters
#[derive(Debug, Clone, Parser)]
pub struct HopParams {
	/// Enable HOP
	#[arg(long)]
	pub enable_hop: bool,

	/// HOP maximum data pool size in MiB
	#[arg(long, default_value = "10240")]
	pub hop_max_pool_size: u64,

	/// HOP data retention period in blocks (24h = 14400 blocks at 6s per block)
	#[arg(long, default_value = "14400")]
	pub hop_retention_blocks: u32,

	/// HOP promotion check interval in seconds
	#[arg(long, default_value = "60")]
	pub hop_check_interval: u64,
}

impl Default for HopParams {
	fn default() -> Self {
		Self {
			enable_hop: false,
			hop_max_pool_size: (DEFAULT_MAX_POOL_SIZE / (1024 * 1024)), // Convert to MiB
			hop_retention_blocks: DEFAULT_RETENTION_BLOCKS,             // 24 hours
			hop_check_interval: 60,                                     // 1 minute
		}
	}
}
