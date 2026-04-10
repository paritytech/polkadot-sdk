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

//! HOP maintenance: periodic promotion of near-expiry pool entries to permanent
//! on-chain storage and cleanup of expired entries.
//!
//! ## Architecture
//!
//! - [`HopPromoter`] — trait for promoting data on-chain (trait-object friendly).
//! - [`RuntimeApiPromoter`] — concrete implementation using [`sp_hop::HopApi`] and
//!   [`sc_transaction_pool_api::LocalTransactionPool`].
//! - [`try_build_promoter`] — detects runtime API support at startup, returns `Some(promoter)` or
//!   logs a warning and returns `None`.
//! - [`HopMaintenanceTask`] — background task combining promotion + cleanup.

use crate::pool::HopDataPool;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_hop::HopApi;
use sp_runtime::{traits::Block as BlockT, AccountId32};
use std::{marker::PhantomData, sync::Arc, time::Duration};

/// Trait for promoting HOP data to permanent on-chain storage.
///
/// Implemented as a trait object so that `HopMaintenanceTask` is not generic
/// over runtime-specific types. The concrete implementation
/// ([`RuntimeApiPromoter`]) uses the `HopApi` runtime API.
pub trait HopPromoter: Send + Sync + 'static {
	/// Promote a blob of HOP data to permanent on-chain storage.
	fn promote(&self, data: Vec<u8>) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Concrete [`HopPromoter`] that uses the [`sp_hop::HopApi`] runtime
/// API to construct general transaction extrinsics and submits them to the local
/// transaction pool.
pub struct RuntimeApiPromoter<Block: BlockT, C, P> {
	client: Arc<C>,
	tx_pool: Arc<P>,
	_phantom: PhantomData<Block>,
}

impl<Block, C, P> RuntimeApiPromoter<Block, C, P>
where
	Block: BlockT,
	C: HeaderBackend<Block> + ProvideRuntimeApi<Block> + Send + Sync + 'static,
	C::Api: sp_hop::HopApi<Block, AccountId32>,
	P: sc_transaction_pool_api::LocalTransactionPool<Block = Block> + 'static,
{
	/// Create a new promoter.
	pub fn new(client: Arc<C>, tx_pool: Arc<P>) -> Self {
		Self { client, tx_pool, _phantom: PhantomData }
	}
}

impl<Block, C, P> HopPromoter for RuntimeApiPromoter<Block, C, P>
where
	Block: BlockT,
	C: HeaderBackend<Block> + ProvideRuntimeApi<Block> + Send + Sync + 'static,
	C::Api: sp_hop::HopApi<Block, AccountId32>,
	P: sc_transaction_pool_api::LocalTransactionPool<Block = Block> + 'static,
{
	fn promote(&self, data: Vec<u8>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		let best_hash = self.client.info().best_hash;
		let ext = self.client.runtime_api().create_promotion_extrinsic(best_hash, data)?;
		self.tx_pool
			.submit_local(best_hash, ext)
			.map_err(|e| format!("submit_local failed: {:?}", e))?;
		Ok(())
	}
}

/// Try to build a [`HopPromoter`] by detecting the `HopApi` runtime
/// API at the current best block.
///
/// Returns `Some(promoter)` if the runtime supports the API, or `None` with a
/// warning log if it doesn't.
pub fn try_build_promoter<Block, C, P>(
	client: &Arc<C>,
	tx_pool: &Arc<P>,
) -> Option<Arc<dyn HopPromoter>>
where
	Block: BlockT,
	C: HeaderBackend<Block> + ProvideRuntimeApi<Block> + Send + Sync + 'static,
	C::Api: sp_hop::HopApi<Block, AccountId32>,
	P: sc_transaction_pool_api::LocalTransactionPool<Block = Block> + 'static,
{
	let best_hash = client.info().best_hash;
	match client
		.runtime_api()
		.has_api_with::<dyn sp_hop::HopApi<Block, AccountId32>, _>(best_hash, |v| v >= 1)
	{
		Ok(true) => {
			tracing::info!(target: "hop", "HopApi detected — promotion enabled");
			Some(Arc::new(RuntimeApiPromoter::new(client.clone(), tx_pool.clone())))
		},
		Ok(false) => {
			tracing::warn!(
				target: "hop",
				"HOP enabled but runtime does not support HopApi — running cleanup only"
			);
			None
		},
		Err(e) => {
			tracing::warn!(
				target: "hop",
				error = %e,
				"Failed to check HopApi support — running cleanup only"
			);
			None
		},
	}
}

/// Background task that periodically promotes near-expiry HOP pool entries to
/// permanent on-chain storage and cleans up expired entries.
pub struct HopMaintenanceTask {
	hop_pool: Arc<HopDataPool>,
	promoter: Option<Arc<dyn HopPromoter>>,
	buffer_blocks: u32,
	check_interval_secs: u64,
	best_block: Arc<dyn Fn() -> u32 + Send + Sync>,
}

impl HopMaintenanceTask {
	/// Create a new maintenance task.
	///
	/// - `promoter`: `Some` to enable on-chain promotion, `None` for cleanup-only.
	/// - `best_block`: closure returning the current best block number.
	/// - `buffer_blocks`: how many blocks before expiry to start promoting.
	/// - `check_interval_secs`: how often to run the maintenance cycle.
	pub fn new(
		hop_pool: Arc<HopDataPool>,
		promoter: Option<Arc<dyn HopPromoter>>,
		best_block: Arc<dyn Fn() -> u32 + Send + Sync>,
		buffer_blocks: u32,
		check_interval_secs: u64,
	) -> Self {
		Self { hop_pool, promoter, buffer_blocks, check_interval_secs, best_block }
	}

	/// Run the maintenance loop.
	pub async fn run(self) {
		loop {
			futures_timer::Delay::new(Duration::from_secs(self.check_interval_secs)).await;
			let current_block = (self.best_block)();

			// Promote near-expiry entries if a promoter is available.
			if let Some(ref promoter) = self.promoter {
				const PROMOTION_BATCH_SIZE: usize = 10;
				let entries = self.hop_pool.get_promotable(
					current_block,
					self.buffer_blocks,
					PROMOTION_BATCH_SIZE,
				);
				for (hash, data) in entries {
					let size = data.len();
					match promoter.promote(data) {
						Ok(()) => {
							self.hop_pool.mark_promoted(&hash);
							tracing::info!(
								target: "hop",
								hash = ?hex::encode(hash),
								size,
								"Promoted HOP entry to on-chain storage"
							);
						},
						Err(e) => {
							tracing::warn!(
								target: "hop",
								hash = ?hex::encode(hash),
								error = %e,
								"Failed to promote HOP entry, will retry"
							);
						},
					}
				}
			}

			// Always clean up expired entries.
			let freed = self.hop_pool.cleanup_expired(current_block);
			if freed > 0 {
				tracing::info!(
					target: "hop",
					freed_bytes = freed,
					"Cleaned up expired HOP entries"
				);
			}
		}
	}
}
