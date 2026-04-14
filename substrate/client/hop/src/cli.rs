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

use crate::types::{
	DEFAULT_CHECK_INTERVAL_SECS, DEFAULT_MAX_POOL_SIZE_MIB, DEFAULT_RETENTION_BLOCKS,
};
use clap::Parser;

/// HOP (Hand-Off Protocol) configuration parameters
#[derive(Debug, Clone, Parser)]
pub struct HopParams {
	/// Enable HOP
	#[arg(long = "enable-hop")]
	pub enabled: bool,

	/// HOP maximum data pool size in MiB
	#[arg(long = "hop-max-pool-size", default_value_t = DEFAULT_MAX_POOL_SIZE_MIB)]
	pub max_pool_size: u64,

	/// HOP data retention period in blocks (24h = 14400 blocks at 6s per block)
	#[arg(long = "hop-retention-blocks", default_value_t = DEFAULT_RETENTION_BLOCKS)]
	pub retention_blocks: u32,

	/// HOP expiry cleanup interval in seconds
	#[arg(long = "hop-check-interval", default_value_t = DEFAULT_CHECK_INTERVAL_SECS)]
	pub check_interval: u64,

	/// Directory for HOP persistent data storage.
	///
	/// If not specified, defaults to `<chain-data-dir>/hop`.
	#[arg(long = "hop-data-dir")]
	pub data_dir: Option<std::path::PathBuf>,
}

impl Default for HopParams {
	fn default() -> Self {
		Self {
			enabled: false,
			max_pool_size: DEFAULT_MAX_POOL_SIZE_MIB,
			retention_blocks: DEFAULT_RETENTION_BLOCKS,
			check_interval: DEFAULT_CHECK_INTERVAL_SECS,
			data_dir: None,
		}
	}
}
