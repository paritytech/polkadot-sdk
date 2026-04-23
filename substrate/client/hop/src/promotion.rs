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
//! - [`RuntimeApiPromoter`] — concrete implementation using [`sp_hop::HopRuntimeApi`] and
//!   [`sc_transaction_pool_api::LocalTransactionPool`].
//! - [`try_build_promoter`] — detects runtime API support at startup, returns `Some(promoter)` or
//!   logs a warning and returns `None`.
//! - [`HopMaintenanceTask`] — background task combining promotion + cleanup.

use crate::pool::HopDataPool;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_hop::HopRuntimeApi;
use sp_runtime::{traits::Block as BlockT, AccountId32, SaturatedConversion};
use std::{marker::PhantomData, sync::Arc, time::Duration};

/// Trait for promoting HOP data to permanent on-chain storage.
///
/// Implemented as a trait object so that `HopMaintenanceTask` is not generic
/// over runtime-specific types. The concrete implementation
/// ([`RuntimeApiPromoter`]) uses the `HopRuntimeApi` runtime API.
pub trait HopPromoter: Send + Sync + 'static {
	/// Promote a blob of HOP data to permanent on-chain storage.
	fn promote(&self, data: Vec<u8>) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Concrete [`HopPromoter`] that uses the [`sp_hop::HopRuntimeApi`] runtime
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
	C::Api: sp_hop::HopRuntimeApi<Block, AccountId32>,
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
	C::Api: sp_hop::HopRuntimeApi<Block, AccountId32>,
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

/// Try to build a [`HopPromoter`] by detecting the `HopRuntimeApi` runtime
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
	C::Api: sp_hop::HopRuntimeApi<Block, AccountId32>,
	P: sc_transaction_pool_api::LocalTransactionPool<Block = Block> + 'static,
{
	let best_hash = client.info().best_hash;
	match client
		.runtime_api()
		.has_api_with::<dyn sp_hop::HopRuntimeApi<Block, AccountId32>, _>(best_hash, |v| v >= 1)
	{
		Ok(true) => {
			tracing::info!(target: "hop", "HopRuntimeApi detected — promotion enabled");
			Some(Arc::new(RuntimeApiPromoter::new(client.clone(), tx_pool.clone())))
		},
		Ok(false) => {
			tracing::warn!(
				target: "hop",
				"HOP enabled but runtime does not support HopRuntimeApi — running cleanup only"
			);
			None
		},
		Err(e) => {
			tracing::warn!(
				target: "hop",
				error = %e,
				"Failed to check HopRuntimeApi support — running cleanup only"
			);
			None
		},
	}
}

/// Build a [`HopMaintenanceTask`] wired to the node's client and transaction pool.
///
/// Detects `HopRuntimeApi` support at startup (see [`try_build_promoter`]) and captures
/// a best-block closure over `client` so callers only need to spawn the returned
/// task on their task manager.
pub fn build_maintenance_task<Block, C, P>(
	client: &Arc<C>,
	tx_pool: &Arc<P>,
	pool: Arc<HopDataPool>,
	buffer_blocks: u32,
	check_interval_secs: u64,
) -> HopMaintenanceTask
where
	Block: BlockT,
	C: HeaderBackend<Block> + ProvideRuntimeApi<Block> + Send + Sync + 'static,
	C::Api: sp_hop::HopRuntimeApi<Block, AccountId32>,
	P: sc_transaction_pool_api::LocalTransactionPool<Block = Block> + 'static,
{
	let promoter = try_build_promoter::<Block, _, _>(client, tx_pool);
	let best_block_client = client.clone();
	let best_block: Arc<dyn Fn() -> u32 + Send + Sync> =
		Arc::new(move || best_block_client.info().best_number.saturated_into::<u32>());
	HopMaintenanceTask::new(pool, promoter, best_block, buffer_blocks, check_interval_secs)
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
			self.tick();
		}
	}

	/// Execute a single maintenance cycle: promote near-expiry entries and clean up expired ones.
	pub fn tick(&self) {
		let current_block = (self.best_block)();

		// Promote near-expiry entries one at a time to bound peak memory.
		if let Some(ref promoter) = self.promoter {
			const PROMOTION_BATCH_SIZE: usize = 10;
			let hashes = self.hop_pool.get_promotable(
				current_block,
				self.buffer_blocks,
				PROMOTION_BATCH_SIZE,
			);
			for hash in hashes {
				let data = match self.hop_pool.get(&hash) {
					Some(data) => data,
					None => continue,
				};
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		pool::HopDataPool,
		rate_limit::RateLimitConfig,
		types::{Recipient, RecipientVec, SenderId},
	};
	use sp_core::{crypto::Pair, ed25519};
	use sp_runtime::MultiSigner;
	use std::sync::Mutex;
	use tempfile::TempDir;

	const SENDER_A: SenderId = [1u8; 32];

	fn test_recipient() -> (ed25519::Pair, MultiSigner) {
		let pair = ed25519::Pair::from_seed(&[1u8; 32]);
		let signer = MultiSigner::Ed25519(pair.public());
		(pair, signer)
	}

	fn bv(v: Vec<MultiSigner>) -> RecipientVec {
		let recipients: Vec<Recipient> =
			v.into_iter().map(|signer| Recipient { signer, claimed: false }).collect();
		RecipientVec::try_from(recipients).expect("test recipient list exceeds MAX_RECIPIENTS")
	}

	fn test_pool(max_size: u64, retention_blocks: u32, dir: &TempDir) -> Arc<HopDataPool> {
		Arc::new(
			HopDataPool::new(
				max_size,
				max_size,
				retention_blocks,
				dir.path().to_path_buf(),
				RateLimitConfig::disabled(),
			)
			.unwrap(),
		)
	}

	struct MockPromoter {
		calls: Mutex<Vec<Vec<u8>>>,
		should_fail: bool,
	}

	impl MockPromoter {
		fn new(should_fail: bool) -> Self {
			Self { calls: Mutex::new(Vec::new()), should_fail }
		}

		fn call_count(&self) -> usize {
			self.calls.lock().unwrap().len()
		}

		fn calls(&self) -> Vec<Vec<u8>> {
			self.calls.lock().unwrap().clone()
		}
	}

	impl HopPromoter for MockPromoter {
		fn promote(&self, data: Vec<u8>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
			self.calls.lock().unwrap().push(data);
			if self.should_fail {
				Err("mock failure".into())
			} else {
				Ok(())
			}
		}
	}

	#[test]
	fn tick_promotes_near_expiry_entries() {
		let dir = TempDir::new().unwrap();
		// retention=100 blocks
		let pool = test_pool(1024 * 1024, 100, &dir);
		let (_, signer) = test_recipient();

		// Insert at block 0, expires at block 100.
		let hash = pool.insert(vec![42u8; 10], 0, bv(vec![signer]), SENDER_A).unwrap();

		let promoter = Arc::new(MockPromoter::new(false));
		let task = HopMaintenanceTask::new(
			pool.clone(),
			Some(promoter.clone()),
			Arc::new(|| 80), // current block = 80
			30,              // buffer = 30 → 80+30=110 >= 100 → promotable
			60,
		);

		task.tick();

		assert_eq!(promoter.call_count(), 1);
		assert_eq!(promoter.calls()[0], vec![42u8; 10]);

		// Entry should be marked as promoted (not removed, just flagged).
		assert!(pool.has(&hash));
		let promotable = pool.get_promotable(80, 30, usize::MAX);
		assert!(promotable.is_empty(), "promoted entry should not be re-promoted");
	}

	#[test]
	fn tick_skips_promotion_when_no_promoter() {
		let dir = TempDir::new().unwrap();
		let pool = test_pool(1024 * 1024, 100, &dir);
		let (_, signer) = test_recipient();

		pool.insert(vec![42u8; 10], 0, bv(vec![signer]), SENDER_A).unwrap();

		let task = HopMaintenanceTask::new(
			pool.clone(),
			None, // no promoter
			Arc::new(|| 80),
			30,
			60,
		);

		task.tick();

		// Entry should still be promotable (no promoter to process it).
		let promotable = pool.get_promotable(80, 30, usize::MAX);
		assert_eq!(promotable.len(), 1);
	}

	#[test]
	fn tick_does_not_mark_promoted_on_failure() {
		let dir = TempDir::new().unwrap();
		let pool = test_pool(1024 * 1024, 100, &dir);
		let (_, signer) = test_recipient();

		pool.insert(vec![42u8; 10], 0, bv(vec![signer]), SENDER_A).unwrap();

		let promoter = Arc::new(MockPromoter::new(true)); // will fail
		let task =
			HopMaintenanceTask::new(pool.clone(), Some(promoter.clone()), Arc::new(|| 80), 30, 60);

		task.tick();

		// Promoter was called but failed.
		assert_eq!(promoter.call_count(), 1);

		// Entry should still be promotable (not marked as promoted).
		let promotable = pool.get_promotable(80, 30, usize::MAX);
		assert_eq!(promotable.len(), 1);
	}

	#[test]
	fn tick_cleans_up_expired_entries() {
		let dir = TempDir::new().unwrap();
		// retention=10 blocks
		let pool = test_pool(1024 * 1024, 10, &dir);
		let (_, signer) = test_recipient();

		pool.insert(vec![42u8; 50], 0, bv(vec![signer]), SENDER_A).unwrap();
		assert_eq!(pool.status().entry_count, 1);

		let task = HopMaintenanceTask::new(
			pool.clone(),
			None,
			Arc::new(|| 10), // block 10 → entry at block 0 with retention 10 is expired
			5,
			60,
		);

		task.tick();

		assert_eq!(pool.status().entry_count, 0);
		assert_eq!(pool.status().total_bytes, 0);
	}

	#[test]
	fn tick_promotes_then_cleans_up_independently() {
		let dir = TempDir::new().unwrap();
		// retention=100
		let pool = test_pool(1024 * 1024, 100, &dir);
		let (_, signer) = test_recipient();

		// Entry A: inserted at block 0, expires at 100 — near expiry at block 80 with buffer 30.
		pool.insert(vec![1u8; 10], 0, bv(vec![signer.clone()]), SENDER_A).unwrap();
		// Entry B: inserted at block 0 with retention 100 but we'll test at block 100 where it's
		// expired. Actually both entries have the same retention. Let's use a different pool.

		// Simpler: one entry near expiry (promotable), one entry expired.
		// We need different retention for different entries, but pool has one retention value.
		// Instead: insert entry at block 0 (expires at 100). At block 80, it's promotable.
		// After promotion, advance to block 100 — it's now expired and should be cleaned.

		let promoter = Arc::new(MockPromoter::new(false));

		// First tick at block 80: promote.
		let block = Arc::new(Mutex::new(80u32));
		let block_clone = block.clone();
		let task = HopMaintenanceTask::new(
			pool.clone(),
			Some(promoter.clone()),
			Arc::new(move || *block_clone.lock().unwrap()),
			30,
			60,
		);

		task.tick();
		assert_eq!(promoter.call_count(), 1);
		assert_eq!(pool.status().entry_count, 1); // still in pool

		// Second tick at block 100: cleanup expired.
		*block.lock().unwrap() = 100;
		task.tick();
		// cleaned up
		assert_eq!(pool.status().entry_count, 0);
		// Promoter not called again (already promoted).
		assert_eq!(promoter.call_count(), 1);
	}
}
