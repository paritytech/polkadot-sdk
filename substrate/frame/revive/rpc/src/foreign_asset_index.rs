// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! In-memory `foreign asset Location -> u32 precompile index` table.
//!
//! The precompile index a foreign asset's ERC-20 address is derived from lives only in chain
//! storage (`AssetsPrecompiles::ForeignAssetIdToAssetIndex`) — it is in no event. Rather than
//! reading that storage per transfer (and at `at_latest`, which made historical resolution
//! time-dependent), this table is **seeded once at startup** and then **kept current from
//! `pallet-assets` lifecycle events**: the mapping is only ever changed by the create/destroy
//! callbacks, which always emit `Created` / `ForceCreated` (add) or `Destroyed` (remove). So
//! resolving a transfer is a pure table lookup, and the only chain reads are the one-time seed
//! plus one read per asset *creation* (the create event omits the assigned index).
//!
//! This deliberately lives outside [`crate::ReceiptExtractor`]: extraction is a pure function of
//! the block, and maintaining a view of mutable chain state is a separate concern owned by the
//! indexing layer, which calls [`ForeignAssetIndex::apply_event`] as it processes each block.

use crate::{AssetTransferConfig, decode_foreign_index, foreign_index_storage_key};
use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

const LOG_TARGET: &str = "eth-rpc::foreign_asset_index";

/// `pallet-assets` event variants that change the foreign `Location -> index` mapping.
const CREATED: &str = "Created";
const FORCE_CREATED: &str = "ForceCreated";
const DESTROYED: &str = "Destroyed";

/// `true` if this `pallet-assets` event variant *adds* a foreign `Location -> index` mapping.
/// The indexing layer applies creations *before* extraction so a same-block transfer resolves.
pub fn is_foreign_creation(variant: &str) -> bool {
	variant == CREATED || variant == FORCE_CREATED
}

/// `true` if this `pallet-assets` event variant *removes* a foreign `Location -> index` mapping.
/// The indexing layer applies destructions *after* extraction, so a transfer earlier in the same
/// block still resolves against the (then-live) mapping.
pub fn is_foreign_destruction(variant: &str) -> bool {
	variant == DESTROYED
}

/// Read a raw storage value. `Ok(Some)` = present, `Ok(None)` = absent, `Err(())` = transient
/// fetch failure. Used to read a newly-created foreign asset's index (which the create event omits)
/// and to seed the table.
pub type FetchStorageRawFn = Arc<
	dyn Fn(Vec<u8>) -> Pin<Box<dyn Future<Output = Result<Option<Vec<u8>>, ()>> + Send>>
		+ Send
		+ Sync,
>;

/// In-memory map of a foreign asset's SCALE-encoded `Location` to its `u32` precompile index.
///
/// Seeded at startup and maintained from lifecycle events; [`Self::get`] is a pure lookup.
#[derive(Clone)]
pub struct ForeignAssetIndex {
	/// `Location (SCALE bytes) -> u32 index`.
	cache: Arc<tokio::sync::Mutex<HashMap<Vec<u8>, u32>>>,
	/// Which metadata pallet names are foreign-assets instances (and their address prefix).
	config: AssetTransferConfig,
	/// Reads `ForeignAssetIdToAssetIndex[Location]` to learn a freshly-created asset's index.
	fetch_index: FetchStorageRawFn,
}

impl ForeignAssetIndex {
	/// Build an empty table. Call [`Self::seed`] before serving to populate it from current state.
	pub fn new(config: AssetTransferConfig, fetch_index: FetchStorageRawFn) -> Self {
		Self { cache: Arc::new(tokio::sync::Mutex::new(HashMap::new())), config, fetch_index }
	}

	/// A genuinely inert table — for tests and mocks that do not exercise foreign-asset resolution.
	/// Empty instance lists so `apply_event` matches nothing, and a resolver that reports "absent"
	/// so `get` is always `None`.
	pub fn disabled() -> Self {
		let config = AssetTransferConfig { u32_instances: vec![], foreign_instances: vec![] };
		Self::new(config, Arc::new(|_| Box::pin(std::future::ready(Ok(None)))))
	}

	/// Look up the precompile index for a foreign asset's SCALE-encoded `Location`. Pure: a table
	/// read, no chain access. Returns `None` if the `Location` is not (yet) known.
	pub async fn get(&self, asset_id_key: &[u8]) -> Option<u32> {
		self.cache.lock().await.get(asset_id_key).copied()
	}

	/// Replace the table with a freshly-read snapshot (startup seed). `entries` is every
	/// `(Location bytes, index)` pair currently in `ForeignAssetIdToAssetIndex`.
	pub async fn seed(&self, entries: impl IntoIterator<Item = (Vec<u8>, u32)>) {
		let mut map = self.cache.lock().await;
		map.clear();
		map.extend(entries);
	}

	/// `true` if `pallet` is a configured foreign-assets instance.
	fn is_foreign_instance(&self, pallet: &str) -> bool {
		self.config.foreign_instances.iter().any(|(name, _)| *name == pallet)
	}

	/// Apply one decoded block event to the table: add on `Created`/`ForceCreated`, remove on
	/// `Destroyed`. A no-op for any other event or for non-foreign pallets. Callers iterate a
	/// block's events (in order) and call this for each.
	///
	/// The create events do not carry the assigned index, so it is read from storage here. A
	/// transient read failure leaves the entry absent; the next seed/restart recovers it.
	pub async fn apply_event(&self, pallet: &str, variant: &str, field_bytes: &[u8]) {
		if !self.is_foreign_instance(pallet) {
			return;
		}
		match variant {
			// Field order: `Created { asset_id: Location, creator, owner }` and
			// `ForceCreated { asset_id: Location, owner }`. `creator`/`owner` are fixed 32-byte
			// `AccountId32`s, so the leading bytes are the SCALE-encoded `Location`.
			CREATED | FORCE_CREATED => {
				let trailing = if variant == CREATED { 64 } else { 32 };
				let Some(split) = field_bytes.len().checked_sub(trailing) else { return };
				let location = &field_bytes[..split];
				match (self.fetch_index)(foreign_index_storage_key(location)).await {
					Ok(Some(raw)) =>
						if let Some(index) = decode_foreign_index(&raw) {
							self.cache.lock().await.insert(location.to_vec(), index);
						} else {
							log::debug!(target: LOG_TARGET, "{pallet}::{variant}: index undecodable, not added");
						},
					Ok(None) => {
						log::debug!(target: LOG_TARGET, "{pallet}::{variant}: no index in storage, not added");
					},
					Err(()) => {
						log::debug!(target: LOG_TARGET, "{pallet}::{variant}: index read failed, not added; reseed/restart recovers");
					},
				}
			},
			// `Destroyed { asset_id: Location }` — the whole field is the `Location`.
			DESTROYED => {
				self.cache.lock().await.remove(field_bytes);
			},
			_ => {},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::atomic::{AtomicU32, Ordering};

	/// A resolver that returns a fixed index (SCALE u32, little-endian) and counts its calls.
	fn fixed_index_resolver(index: u32) -> (FetchStorageRawFn, Arc<AtomicU32>) {
		let calls = Arc::new(AtomicU32::new(0));
		let calls_in = calls.clone();
		let resolver: FetchStorageRawFn = Arc::new(move |_key: Vec<u8>| {
			calls_in.fetch_add(1, Ordering::SeqCst);
			let raw = index.to_le_bytes().to_vec();
			Box::pin(std::future::ready(Ok(Some(raw)))) as Pin<Box<_>>
		});
		(resolver, calls)
	}

	#[test]
	fn seed_then_lookup() {
		let (resolver, _) = fixed_index_resolver(0);
		let index = ForeignAssetIndex::new(AssetTransferConfig::default(), resolver);
		futures::executor::block_on(async {
			index.seed([(vec![1, 2, 3], 7), (vec![4, 5], 9)]).await;
			assert_eq!(index.get(&[1, 2, 3]).await, Some(7));
			assert_eq!(index.get(&[4, 5]).await, Some(9));
			assert_eq!(index.get(&[9, 9]).await, None);
		});
	}

	#[test]
	fn created_adds_force_created_adds_destroyed_removes() {
		let (resolver, calls) = fixed_index_resolver(42);
		let index = ForeignAssetIndex::new(AssetTransferConfig::default(), resolver);
		let location = vec![0x01, 0x02, 0xCA, 0xFE];

		futures::executor::block_on(async {
			// `Created { location, creator(32), owner(32) }` -> trailing 64 bytes stripped.
			let mut created = location.clone();
			created.extend_from_slice(&[0xAA; 64]);
			index.apply_event("ForeignAssets", "Created", &created).await;
			assert_eq!(index.get(&location).await, Some(42), "Created adds the mapping");

			// Destroyed removes it.
			index.apply_event("ForeignAssets", "Destroyed", &location).await;
			assert_eq!(index.get(&location).await, None, "Destroyed removes the mapping");

			// `ForceCreated { location, owner(32) }` -> trailing 32 bytes stripped.
			let mut force_created = location.clone();
			force_created.extend_from_slice(&[0xBB; 32]);
			index.apply_event("ForeignAssets", "ForceCreated", &force_created).await;
			assert_eq!(index.get(&location).await, Some(42), "ForceCreated adds the mapping");
		});
		// One storage read per creation event (Created + ForceCreated); Destroyed reads nothing.
		assert_eq!(calls.load(Ordering::SeqCst), 2);
	}

	/// A resolver that returns a queue of canned responses (one per call, in order); once drained
	/// it returns `Ok(None)`. Lets a test vary the resolved index across successive create events.
	fn queued_resolver(
		responses: Vec<Result<Option<Vec<u8>>, ()>>,
	) -> (FetchStorageRawFn, Arc<AtomicU32>) {
		use std::{collections::VecDeque, sync::Mutex};
		let calls = Arc::new(AtomicU32::new(0));
		let calls_in = calls.clone();
		let queue = Arc::new(Mutex::new(VecDeque::from(responses)));
		let resolver: FetchStorageRawFn = Arc::new(move |_key: Vec<u8>| {
			calls_in.fetch_add(1, Ordering::SeqCst);
			let out = queue.lock().unwrap().pop_front().unwrap_or(Ok(None));
			Box::pin(std::future::ready(out)) as Pin<Box<_>>
		});
		(resolver, calls)
	}

	#[test]
	fn destroyed_then_recreated_uses_new_index() {
		// First creation resolves to index 7; after destruction, a re-creation resolves to a *new*
		// index 8 (NextAssetIndex is monotonic). The stale 7 must not linger.
		let (resolver, _) = queued_resolver(vec![
			Ok(Some(7u32.to_le_bytes().to_vec())),
			Ok(Some(8u32.to_le_bytes().to_vec())),
		]);
		let index = ForeignAssetIndex::new(AssetTransferConfig::default(), resolver);
		let location = vec![0xDE, 0xAD];
		let mut created = location.clone();
		created.extend_from_slice(&[0u8; 64]);

		futures::executor::block_on(async {
			index.apply_event("ForeignAssets", "Created", &created).await;
			assert_eq!(index.get(&location).await, Some(7));

			index.apply_event("ForeignAssets", "Destroyed", &location).await;
			assert_eq!(index.get(&location).await, None);

			index.apply_event("ForeignAssets", "Created", &created).await;
			assert_eq!(
				index.get(&location).await,
				Some(8),
				"re-creation must use the new index, not the stale one"
			);
		});
	}

	#[test]
	fn apply_event_does_not_add_on_absent_or_undecodable_index() {
		// Call 0: `Ok(None)` (mapping not in storage). Call 1: `Ok(Some(undecodable))` (3 bytes, not
		// a u32). Neither is a resolved index, so neither is added — the table holds resolved
		// indices only, so a later real creation can still populate it.
		let (resolver, calls) =
			queued_resolver(vec![Ok(None), Ok(Some(vec![1, 2, 3]))]);
		let index = ForeignAssetIndex::new(AssetTransferConfig::default(), resolver);
		let location = vec![0x11];
		let mut created = location.clone();
		created.extend_from_slice(&[0u8; 64]);

		futures::executor::block_on(async {
			index.apply_event("ForeignAssets", "Created", &created).await; // Ok(None)
			assert_eq!(index.get(&location).await, None, "absent index must not be added");
			index.apply_event("ForeignAssets", "Created", &created).await; // undecodable
			assert_eq!(index.get(&location).await, None, "undecodable index must not be added");
		});
		assert_eq!(calls.load(Ordering::SeqCst), 2);
	}

	#[test]
	fn maintains_every_configured_foreign_instance() {
		// A runtime can wire more than one foreign-assets instance; all configured ones are tracked.
		let (resolver, _) = fixed_index_resolver(3);
		let config = AssetTransferConfig {
			foreign_instances: vec![("ForeignAssets", 0x0220), ("OtherForeign", 0x0420)],
			..AssetTransferConfig::default()
		};
		let index = ForeignAssetIndex::new(config, resolver);
		let location = vec![0x55];
		let mut created = location.clone();
		created.extend_from_slice(&[0u8; 64]);

		futures::executor::block_on(async {
			index.apply_event("OtherForeign", "Created", &created).await;
			assert_eq!(index.get(&location).await, Some(3), "second foreign instance is tracked");
		});
	}

	#[test]
	fn ignores_unrelated_pallets_and_variants() {
		let (resolver, calls) = fixed_index_resolver(1);
		let index = ForeignAssetIndex::new(AssetTransferConfig::default(), resolver);
		let location = vec![0x07];
		let mut created = location.clone();
		created.extend_from_slice(&[0u8; 64]);

		futures::executor::block_on(async {
			// `Assets` is a u32-id instance, not a foreign instance -> ignored.
			index.apply_event("Assets", "Created", &created).await;
			// Unrelated variant on a foreign instance -> ignored.
			index.apply_event("ForeignAssets", "Issued", &created).await;
			assert_eq!(index.get(&location).await, None);
		});
		assert_eq!(calls.load(Ordering::SeqCst), 0, "no storage read for ignored events");
	}

	// ---- repro tests for the fixes from the external review ----

	// Same-block destroy ordering (review Medium #1): the indexing layer classifies events with
	// `is_foreign_creation` / `is_foreign_destruction` so it can apply creations BEFORE extraction
	// and destructions AFTER. These predicates are what route each variant to the right phase.
	#[test]
	fn event_variant_phase_classification() {
		assert!(is_foreign_creation("Created"));
		assert!(is_foreign_creation("ForceCreated"));
		assert!(!is_foreign_creation("Destroyed"));
		assert!(!is_foreign_creation("Transferred"));

		assert!(is_foreign_destruction("Destroyed"));
		assert!(!is_foreign_destruction("Created"));
		assert!(!is_foreign_destruction("ForceCreated"));
	}

	// Same-block destroy ordering (review Medium #1): a transfer earlier in a block must still
	// resolve when the asset is destroyed later in the SAME block. The provider applies
	// destructions only AFTER extraction, so at extraction time the mapping is still present; the
	// destruction then takes effect for subsequent blocks. (The buggy "apply all events before
	// extraction" would have removed the mapping before the transfer could resolve.)
	#[test]
	fn destruction_applied_after_extraction_preserves_same_block_transfer() {
		let (resolver, _) = fixed_index_resolver(0);
		let index = ForeignAssetIndex::new(AssetTransferConfig::default(), resolver);
		let location = vec![0xCA, 0xFE];

		futures::executor::block_on(async {
			index.seed([(location.clone(), 5)]).await;

			// Extraction-time read (the transfer at extrinsic i): resolves against the live mapping.
			assert_eq!(index.get(&location).await, Some(5), "transfer resolves at extraction time");

			// Destruction (extrinsic j > i) is applied only now, after extraction.
			index.apply_event("ForeignAssets", "Destroyed", &location).await;
			assert_eq!(index.get(&location).await, None, "destruction takes effect afterwards");
		});
	}

	// disabled() must be genuinely inert (review minor): empty instance lists, so `apply_event`
	// matches nothing even for a real foreign pallet name, and `get` is always None.
	#[test]
	fn disabled_is_truly_inert() {
		let index = ForeignAssetIndex::disabled();
		let location = vec![0x01];
		let mut created = location.clone();
		created.extend_from_slice(&[0u8; 64]);

		futures::executor::block_on(async {
			index.apply_event("ForeignAssets", "Created", &created).await;
			assert_eq!(index.get(&location).await, None, "disabled() never adds, even for ForeignAssets");
		});
	}

	// And the underlying mechanism: an empty `foreign_instances` config matches no pallet, so even
	// a resolver that WOULD return an index never adds one.
	#[test]
	fn empty_foreign_config_matches_nothing() {
		let (resolver, calls) = fixed_index_resolver(7);
		let config = AssetTransferConfig { u32_instances: vec![], foreign_instances: vec![] };
		let index = ForeignAssetIndex::new(config, resolver);
		let location = vec![0x02];
		let mut created = location.clone();
		created.extend_from_slice(&[0u8; 64]);

		futures::executor::block_on(async {
			index.apply_event("ForeignAssets", "Created", &created).await;
			assert_eq!(index.get(&location).await, None);
		});
		assert_eq!(calls.load(Ordering::SeqCst), 0, "no resolver call when nothing matches");
	}

	// Read purity (supports review Blocking): `get` never mutates the table — the read primitive
	// the extractor uses is side-effect-free, so a read path can't change shared state through it.
	// (The end-to-end "receipts_from_block doesn't mutate" guarantee is structural — maintenance is
	// only invoked from the forward indexing path — and is exercised by the Tier 3 fixture.)
	#[test]
	fn get_does_not_mutate() {
		let (resolver, _) = fixed_index_resolver(0);
		let index = ForeignAssetIndex::new(AssetTransferConfig::default(), resolver);

		futures::executor::block_on(async {
			index.seed([(vec![1, 2, 3], 9)]).await;
			for _ in 0..5 {
				let _ = index.get(&[1, 2, 3]).await; // hit
				let _ = index.get(&[9, 9, 9]).await; // miss
			}
			assert_eq!(index.get(&[1, 2, 3]).await, Some(9), "hits unchanged after repeated reads");
			assert_eq!(index.get(&[9, 9, 9]).await, None, "misses never insert");
		});
	}
}
