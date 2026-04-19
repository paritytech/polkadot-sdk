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

use crate::{
	rate_limit::RateLimitConfig,
	types::{
		DEFAULT_BANDWIDTH_BURST_MIB, DEFAULT_BANDWIDTH_PER_MIN_MIB, DEFAULT_CHECK_INTERVAL_SECS,
		DEFAULT_MAX_POOL_SIZE_MIB, DEFAULT_MAX_USER_SIZE_MIB, DEFAULT_PROMOTION_BUFFER_BLOCKS,
		DEFAULT_RETENTION_BLOCKS, DEFAULT_SUBMIT_BURST, DEFAULT_SUBMIT_RATE_PER_MIN,
	},
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

	/// HOP maximum per-user pool size in MiB (hard cap, not scaled by active users)
	#[arg(long = "hop-max-user-size", default_value_t = DEFAULT_MAX_USER_SIZE_MIB)]
	pub max_user_size: u64,

	/// HOP data retention period in blocks (24h = 14400 blocks at 6s per block)
	#[arg(long = "hop-retention-blocks", default_value_t = DEFAULT_RETENTION_BLOCKS)]
	pub retention_blocks: u32,

	/// HOP expiry cleanup interval in seconds
	#[arg(long = "hop-check-interval", default_value_t = DEFAULT_CHECK_INTERVAL_SECS)]
	pub check_interval: u64,

	/// Blocks before expiry at which to start promoting entries on-chain
	#[arg(long = "hop-promotion-buffer-blocks", default_value_t = DEFAULT_PROMOTION_BUFFER_BLOCKS)]
	pub promotion_buffer_blocks: u32,

	/// Sustained per-account submit rate (requests per minute)
	#[arg(long = "hop-submit-rate-per-min", default_value_t = DEFAULT_SUBMIT_RATE_PER_MIN)]
	pub submit_rate_per_min: u32,

	/// Per-account submit burst size (requests)
	#[arg(long = "hop-submit-burst", default_value_t = DEFAULT_SUBMIT_BURST)]
	pub submit_burst: u32,

	/// Sustained per-account bandwidth (MiB per minute)
	#[arg(long = "hop-bandwidth-per-min-mib", default_value_t = DEFAULT_BANDWIDTH_PER_MIN_MIB)]
	pub bandwidth_per_min_mib: u64,

	/// Per-account bandwidth burst size (MiB)
	#[arg(long = "hop-bandwidth-burst-mib", default_value_t = DEFAULT_BANDWIDTH_BURST_MIB)]
	pub bandwidth_burst_mib: u64,

	/// Disable per-account submit rate limiting (intended for tests and dev nodes).
	#[arg(long = "hop-disable-rate-limit")]
	pub disable_rate_limit: bool,

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
			max_user_size: DEFAULT_MAX_USER_SIZE_MIB,
			retention_blocks: DEFAULT_RETENTION_BLOCKS,
			check_interval: DEFAULT_CHECK_INTERVAL_SECS,
			promotion_buffer_blocks: DEFAULT_PROMOTION_BUFFER_BLOCKS,
			submit_rate_per_min: DEFAULT_SUBMIT_RATE_PER_MIN,
			submit_burst: DEFAULT_SUBMIT_BURST,
			bandwidth_per_min_mib: DEFAULT_BANDWIDTH_PER_MIN_MIB,
			bandwidth_burst_mib: DEFAULT_BANDWIDTH_BURST_MIB,
			disable_rate_limit: false,
			data_dir: None,
		}
	}
}

impl HopParams {
	/// Derive a [`RateLimitConfig`] from these CLI parameters.
	pub fn rate_limit_config(&self) -> RateLimitConfig {
		if self.disable_rate_limit {
			return RateLimitConfig::disabled();
		}
		RateLimitConfig {
			enabled: true,
			submit_rate_per_min: self.submit_rate_per_min,
			submit_burst: self.submit_burst,
			bandwidth_per_min: self.bandwidth_per_min_mib.saturating_mul(1024 * 1024),
			bandwidth_burst: self.bandwidth_burst_mib.saturating_mul(1024 * 1024),
		}
	}
}
