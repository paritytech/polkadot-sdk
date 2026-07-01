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

//! In-memory journal of a foreign asset's `Location -> u32 precompile index` mapping *over time*.
//!
//! The precompile index a foreign asset's ERC-20 address is derived from lives only in chain
//! storage (`AssetsPrecompiles::ForeignAssetIdToAssetIndex`) — it is in no event. A transfer in
//! block `N` must be resolved against the mapping **as it was at block `N`**, not the current one:
//! an asset can be destroyed (and the `Location` later recreated with a *different* index), so a
//! current-state table would mis-resolve historic transfers.
//!
//! So instead of a current-state map, this keeps a per-`Location` journal of mapping *changes*,
//! each stamped with the block it took effect at:
//!
//! ```text
//! Location -> { block_number -> Option<index> }   // Some = (re)created, None = destroyed/absent
//! ```
//!
//! Resolving a transfer at block `N` is the greatest journal entry with `block_number <= N`. A
//! journal entry is a *fact* keyed by block, so the order entries are inserted in is irrelevant —
//! forward live-indexing and backward historic backfill can populate it in any order. Entries come
//! from `pallet-assets` lifecycle events (`Created`/`ForceCreated` add, `Destroyed` removes), each
//! stamped at the block that emitted it, plus a startup cutover seed in pruned/live mode.
//!
//! This deliberately lives outside [`crate::ReceiptExtractor`]: lookups
//! ([`ForeignAssetIndex::get`]) are a pure journal read with no chain access, so extraction stays a
//! pure function of the block. Maintaining the journal is a separate concern owned by the indexing
//! layer, which calls [`ForeignAssetIndex::apply_event`] as it processes each block (the create
//! events omit the assigned index, so *that* — and only that — reads chain storage, on the
//! write/index path).

use crate::{
	AssetTransferConfig,
	client::{SubstrateBlockHash, SubstrateBlockNumber},
};
use codec::Decode;
use sp_crypto_hashing::{blake2_128, twox_128};
use std::{
	collections::{BTreeMap, HashMap},
	future::Future,
	pin::Pin,
	sync::Arc,
};

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

// ---- `AssetsPrecompiles::ForeignAssetIdToAssetIndex` storage-map codec ----
//
// The foreign `Location -> u32 index` mapping lives only in this on-chain map. These build the raw
// storage keys the journal reads (via its `fetch_index` accessor) and that the pruned/live cutover
// seed enumerates by prefix, plus decode the raw value.

/// Storage location of the foreign-asset `Location -> u32 index` map, in the
/// `pallet-assets-precompiles` index pallet. Fixed across runtimes that wire that pallet.
const FOREIGN_INDEX_PALLET: &str = "AssetsPrecompiles";
const FOREIGN_INDEX_ENTRY: &str = "ForeignAssetIdToAssetIndex";

/// The 32-byte storage prefix (`twox128(pallet) ++ twox128(entry)`) of the
/// `FOREIGN_INDEX_PALLET::FOREIGN_INDEX_ENTRY` map — the leading bytes of every key in it (see
/// [`foreign_index_storage_key`]).
pub fn foreign_index_prefix() -> Vec<u8> {
	let mut prefix = Vec::with_capacity(16 + 16);
	prefix.extend_from_slice(&twox_128(FOREIGN_INDEX_PALLET.as_bytes()));
	prefix.extend_from_slice(&twox_128(FOREIGN_INDEX_ENTRY.as_bytes()));
	prefix
}

/// Build the full storage key for the `Blake2_128Concat`
/// `FOREIGN_INDEX_PALLET::FOREIGN_INDEX_ENTRY` map, keyed by the SCALE-encoded asset `Location`.
/// The eth-rpc reads this raw key via `fetch_raw` to resolve a foreign asset's `u32` index.
pub fn foreign_index_storage_key(asset_id_key: &[u8]) -> Vec<u8> {
	let mut key = foreign_index_prefix();
	key.reserve(16 + asset_id_key.len());
	key.extend_from_slice(&blake2_128(asset_id_key));
	key.extend_from_slice(asset_id_key);
	key
}

/// Recover the SCALE-encoded `Location` from a full `ForeignAssetIdToAssetIndex` storage key, by
/// stripping the 32-byte map prefix and the 16-byte `Blake2_128` hash (`Blake2_128Concat` appends
/// the raw key after its hash). Returns `None` if the key is too short to be one of this map's.
/// Used by the pruned/live-mode cutover seed, which enumerates the map by prefix.
pub fn location_from_foreign_index_key(full_key: &[u8]) -> Option<Vec<u8>> {
	// twox128(pallet) ++ twox128(entry) ++ blake2_128(loc) ++ loc
	full_key.get(48..).map(|loc| loc.to_vec())
}

/// Decode the `u32` asset index from a raw `ForeignAssetIdToAssetIndex` storage value.
pub fn decode_foreign_index(mut raw: &[u8]) -> Option<u32> {
	u32::decode(&mut raw).ok()
}

/// Read a raw storage value **at a given block**. `Ok(Some)` = present, `Ok(None)` = absent,
/// `Err(())` = transient fetch failure. Used to read a newly-created foreign asset's index (which
/// the create event omits) at the block it was created, and — during archive backfill — to resolve
/// a transfer's mapping as it was at the block being indexed (see [`Self::resolve_from_storage`]).
pub type FetchStorageRawFn = Arc<
	dyn Fn(
			Vec<u8>,
			SubstrateBlockHash,
		) -> Pin<Box<dyn Future<Output = Result<Option<Vec<u8>>, ()>> + Send>>
		+ Send
		+ Sync,
>;

/// In-memory journal of a foreign asset's SCALE-encoded `Location` to its `u32` precompile index
/// *over block height*. See the module docs: resolution is the latest change at or before a block.
#[derive(Clone)]
pub struct ForeignAssetIndex {
	/// `Location (SCALE bytes) -> { block_number -> Option<u32> }`. Each entry records the mapping
	/// as of that block: `Some(index)` on (re)creation, `None` on destruction (or read-as-absent).
	journal: Arc<tokio::sync::Mutex<HashMap<Vec<u8>, BTreeMap<SubstrateBlockNumber, Option<u32>>>>>,
	/// Which metadata pallet names are foreign-assets instances (and their address prefix).
	config: AssetTransferConfig,
	/// Reads `ForeignAssetIdToAssetIndex[Location]` at a given block — to learn a freshly-created
	/// asset's index (for the journal), and, during archive backfill, to resolve a transfer's
	/// mapping as of the block being indexed (see [`Self::resolve_from_storage`]).
	fetch_index: FetchStorageRawFn,
}

impl ForeignAssetIndex {
	/// Build an empty journal. Populated from `pallet-assets` lifecycle events (and, in pruned/live
	/// mode, a startup cutover seed); archive backfill resolves its logs via
	/// [`Self::resolve_from_storage`].
	pub fn new(config: AssetTransferConfig, fetch_index: FetchStorageRawFn) -> Self {
		Self { journal: Arc::new(tokio::sync::Mutex::new(HashMap::new())), config, fetch_index }
	}

	/// A genuinely inert journal — for tests and mocks that do not exercise foreign-asset
	/// resolution. Empty instance lists so `apply_event` matches nothing, and a resolver that
	/// reports "absent" so a lookup miss resolves to `None`.
	pub fn disabled() -> Self {
		let config = AssetTransferConfig { u32_instances: vec![], foreign_instances: vec![] };
		Self::new(config, Arc::new(|_, _| Box::pin(std::future::ready(Ok(None)))))
	}

	/// Seed the journal with the mappings live at `block`, each recorded as `Some(index)` stamped
	/// at `block`. Used once at startup in **pruned/live mode**, where indexing begins at a live
	/// cutover block rather than reconstructing history: this establishes a deterministic baseline
	/// at the cutover, after which forward lifecycle events maintain the journal. **Archive mode
	/// does not seed** — it resolves historic logs from storage at the block (see
	/// [`Self::resolve_from_storage`]), so a current-state seed would pollute resolution. Entries
	/// are stamped at the cutover, so a transfer at any later block resolves against them unless a
	/// forward event supersedes.
	pub async fn seed_at_block(
		&self,
		block: SubstrateBlockNumber,
		entries: impl IntoIterator<Item = (Vec<u8>, u32)>,
	) {
		let mut journal = self.journal.lock().await;
		for (location, index) in entries {
			journal.entry(location).or_default().insert(block, Some(index));
		}
	}

	/// Look up the precompile index for a foreign asset's SCALE-encoded `Location` **as of
	/// `at_block`**: the latest recorded change at or before `at_block`. A pure journal lookup — no
	/// chain access — so extraction stays a pure function of the block. Returns `None` if the
	/// `Location` had no mapping at that height (the journal is populated by the indexing layer via
	/// [`Self::apply_event`] as it processes each block, plus the startup cutover seed).
	pub async fn get(&self, asset_id_key: &[u8], at_block: SubstrateBlockNumber) -> Option<u32> {
		let journal = self.journal.lock().await;
		let history = journal.get(asset_id_key)?;
		let (_, mapping) = history.range(..=at_block).next_back()?;
		*mapping
	}

	/// Forget every journal change stamped at one of `blocks`. Called when those heights are
	/// orphaned by a reorg (see [`crate::ReceiptProvider::prune_blocks`]): the fork's `Created`/
	/// `Destroyed` facts must not keep resolving canonical-chain transfers. A `Location` left with
	/// no remaining changes is dropped entirely. The canonical block at each height re-records its
	/// own facts when it is indexed, so this only removes the dead fork's view.
	pub async fn forget_blocks(&self, blocks: &[SubstrateBlockNumber]) {
		if blocks.is_empty() {
			return;
		}
		let mut journal = self.journal.lock().await;
		journal.retain(|_location, history| {
			history.retain(|block, _| !blocks.contains(block));
			!history.is_empty()
		});
	}

	/// Record that, as of `block`, `location` mapped to `mapping` (`Some(index)` / `None`).
	async fn record_change(
		&self,
		location: Vec<u8>,
		block: SubstrateBlockNumber,
		mapping: Option<u32>,
	) {
		self.journal.lock().await.entry(location).or_default().insert(block, mapping);
	}

	/// `true` if `pallet` is a configured foreign-assets instance.
	fn is_foreign_instance(&self, pallet: &str) -> bool {
		self.config.foreign_instances.iter().any(|(name, _)| *name == pallet)
	}

	/// Apply one decoded block event to the journal, stamped at `block`: record a `Some(index)` on
	/// `Created`/`ForceCreated`, a `None` on `Destroyed`. A no-op for any other event or for
	/// non-foreign pallets. Callers iterate a block's events (in order) and call this for each,
	/// with the block's number and hash.
	///
	/// The create events do not carry the assigned index, so it is read from storage **at this
	/// block** (`block_hash`) — reading at the creating block, not `at_latest`, so a `Location`
	/// recreated with a different index records the index it had at the time. A transient read
	/// failure records nothing; a later lookup-miss read recovers it.
	pub async fn apply_event(
		&self,
		pallet: &str,
		variant: &str,
		field_bytes: &[u8],
		block: SubstrateBlockNumber,
		block_hash: SubstrateBlockHash,
	) {
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
				match (self.fetch_index)(foreign_index_storage_key(location), block_hash).await {
					Ok(Some(raw)) => {
						if let Some(index) = decode_foreign_index(&raw) {
							self.record_change(location.to_vec(), block, Some(index)).await;
						} else {
							log::debug!(target: LOG_TARGET, "{pallet}::{variant} #{block}: index undecodable, not recorded");
						}
					},
					Ok(None) => {
						log::debug!(target: LOG_TARGET, "{pallet}::{variant} #{block}: no index in storage, not recorded");
					},
					Err(()) => {
						log::debug!(target: LOG_TARGET, "{pallet}::{variant} #{block}: index read failed, not recorded; miss read recovers");
					},
				}
			},
			// `Destroyed { asset_id: Location }` — the whole field is the `Location`.
			DESTROYED => {
				self.record_change(field_bytes.to_vec(), block, None).await;
			},
			_ => {},
		}
	}

	/// Resolve a foreign asset's precompile index by reading `ForeignAssetIdToAssetIndex[Location]`
	/// directly from chain storage **at `block_hash`**, bypassing the journal entirely.
	///
	/// Used for archive backfill log construction: a historic block's synthetic ERC-20 logs must
	/// reflect the mapping as it was at that block, and the in-memory journal is not the source of
	/// truth for historic reconstruction (it may not yet be populated, and is built for live/read
	/// lookups). Returns `None` if the mapping is absent, undecodable, or the read fails.
	pub async fn resolve_from_storage(
		&self,
		asset_id_key: &[u8],
		block_hash: SubstrateBlockHash,
	) -> Option<u32> {
		match (self.fetch_index)(foreign_index_storage_key(asset_id_key), block_hash).await {
			Ok(Some(raw)) => decode_foreign_index(&raw),
			Ok(None) | Err(()) => None,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::atomic::{AtomicU32, Ordering};

	/// A distinct block hash per `n`, so a storage-aware resolver can answer by block.
	fn hash(n: u64) -> SubstrateBlockHash {
		SubstrateBlockHash::from_low_u64_be(n)
	}

	/// A resolver that returns a fixed index (SCALE u32, little-endian) for any key/block and
	/// counts its calls.
	fn fixed_index_resolver(index: u32) -> (FetchStorageRawFn, Arc<AtomicU32>) {
		let calls = Arc::new(AtomicU32::new(0));
		let calls_in = calls.clone();
		let resolver: FetchStorageRawFn =
			Arc::new(move |_key: Vec<u8>, _at: SubstrateBlockHash| {
				calls_in.fetch_add(1, Ordering::SeqCst);
				let raw = index.to_le_bytes().to_vec();
				Box::pin(std::future::ready(Ok(Some(raw)))) as Pin<Box<_>>
			});
		(resolver, calls)
	}

	/// A resolver that always reports the mapping as absent (`Ok(None)`) and counts its calls. Lets
	/// a test observe lookup misses without the resolver inventing an index.
	fn absent_resolver() -> (FetchStorageRawFn, Arc<AtomicU32>) {
		let calls = Arc::new(AtomicU32::new(0));
		let calls_in = calls.clone();
		let resolver: FetchStorageRawFn =
			Arc::new(move |_key: Vec<u8>, _at: SubstrateBlockHash| {
				calls_in.fetch_add(1, Ordering::SeqCst);
				Box::pin(std::future::ready(Ok(None))) as Pin<Box<_>>
			});
		(resolver, calls)
	}

	/// A resolver that returns a queue of canned responses (one per call, in order); once drained
	/// it returns `Ok(None)`. Lets a test vary the resolved index across successive reads.
	fn queued_resolver(
		responses: Vec<Result<Option<Vec<u8>>, ()>>,
	) -> (FetchStorageRawFn, Arc<AtomicU32>) {
		use std::{collections::VecDeque, sync::Mutex};
		let calls = Arc::new(AtomicU32::new(0));
		let calls_in = calls.clone();
		let queue = Arc::new(Mutex::new(VecDeque::from(responses)));
		let resolver: FetchStorageRawFn =
			Arc::new(move |_key: Vec<u8>, _at: SubstrateBlockHash| {
				calls_in.fetch_add(1, Ordering::SeqCst);
				let out = queue.lock().unwrap().pop_front().unwrap_or(Ok(None));
				Box::pin(std::future::ready(out)) as Pin<Box<_>>
			});
		(resolver, calls)
	}

	/// The core of the journal: a transfer at block `N` resolves to the latest change at or before
	/// `N`. Entries are inserted directly (order-independent), and `absent_resolver` makes any
	/// genuine miss observable as `None`.
	#[test]
	fn resolves_latest_change_at_or_before_block() {
		let (resolver, _) = absent_resolver();
		let index = ForeignAssetIndex::new(AssetTransferConfig::default(), resolver);
		let loc = vec![0xCA, 0xFE];

		futures::executor::block_on(async {
			// Created@10 -> 7, Destroyed@20, recreated@30 -> 8.
			index.record_change(loc.clone(), 10, Some(7)).await;
			index.record_change(loc.clone(), 20, None).await;
			index.record_change(loc.clone(), 30, Some(8)).await;

			assert_eq!(index.get(&loc, 9).await, None, "before creation: absent");
			assert_eq!(index.get(&loc, 10).await, Some(7), "at creation");
			assert_eq!(index.get(&loc, 15).await, Some(7), "between create and destroy");
			assert_eq!(index.get(&loc, 20).await, None, "at destruction");
			assert_eq!(index.get(&loc, 25).await, None, "after destruction");
			assert_eq!(index.get(&loc, 30).await, Some(8), "at recreation: new index");
			assert_eq!(index.get(&loc, 99).await, Some(8), "after recreation");
		});
	}

	/// A lookup is pure: an unknown `Location` (or a block before any recorded change) resolves to
	/// `None`, and `get` never invokes the resolver — extraction does no chain I/O.
	#[test]
	fn lookup_is_pure_and_never_reads_chain() {
		let (resolver, calls) = fixed_index_resolver(7);
		let index = ForeignAssetIndex::new(AssetTransferConfig::default(), resolver);
		let loc = vec![0x01];

		futures::executor::block_on(async {
			assert_eq!(index.get(&loc, 50).await, None, "unknown location resolves to None");
			index.record_change(loc.clone(), 40, Some(7)).await;
			assert_eq!(index.get(&loc, 39).await, None, "before the recorded change: None");
			assert_eq!(index.get(&loc, 50).await, Some(7), "at/after the recorded change");
			assert_eq!(calls.load(Ordering::SeqCst), 0, "get never invokes the resolver");
		});
	}

	/// `Created`/`ForceCreated` record the index read at the *event's* block; `Destroyed` records a
	/// `None`. Reading at the event block (not `at_latest`) is what lets a recreated `Location`
	/// keep distinct historic indices.
	#[test]
	fn create_records_index_at_event_block_destroy_records_none() {
		let (resolver, calls) = queued_resolver(vec![
			Ok(Some(7u32.to_le_bytes().to_vec())),
			Ok(Some(8u32.to_le_bytes().to_vec())),
		]);
		let index = ForeignAssetIndex::new(AssetTransferConfig::default(), resolver);
		let loc = vec![0xDE, 0xAD];
		let mut created = loc.clone();
		created.extend_from_slice(&[0u8; 64]);
		let mut force_created = loc.clone();
		force_created.extend_from_slice(&[0u8; 32]);

		futures::executor::block_on(async {
			index.apply_event("ForeignAssets", "Created", &created, 10, hash(10)).await;
			index.apply_event("ForeignAssets", "Destroyed", &loc, 20, hash(20)).await;
			index
				.apply_event("ForeignAssets", "ForceCreated", &force_created, 30, hash(30))
				.await;

			// One read per creation (Created + ForceCreated); Destroyed reads nothing.
			assert_eq!(calls.load(Ordering::SeqCst), 2);

			assert_eq!(index.get(&loc, 15).await, Some(7), "historic index preserved");
			assert_eq!(index.get(&loc, 25).await, None, "destroyed window");
			assert_eq!(index.get(&loc, 35).await, Some(8), "recreated with new index");
		});
	}

	/// A create whose index is absent or undecodable in storage records nothing — the journal holds
	/// resolved changes only, so a later (miss) read can still populate it.
	#[test]
	fn create_with_absent_or_undecodable_index_records_nothing() {
		let (resolver, calls) = queued_resolver(vec![Ok(None), Ok(Some(vec![1, 2, 3]))]);
		let index = ForeignAssetIndex::new(AssetTransferConfig::default(), resolver);
		let loc = vec![0x11];
		let mut created = loc.clone();
		created.extend_from_slice(&[0u8; 64]);

		futures::executor::block_on(async {
			index.apply_event("ForeignAssets", "Created", &created, 10, hash(10)).await; // Ok(None)
			index.apply_event("ForeignAssets", "Created", &created, 11, hash(11)).await; // undecodable
			assert_eq!(calls.load(Ordering::SeqCst), 2);
			assert!(
				index.journal.lock().await.get(&loc).is_none(),
				"nothing recorded for an absent/undecodable create index"
			);
		});
	}

	/// Destructions are applied *after* extraction: a transfer earlier in a block resolves against
	/// the then-live mapping, and the `None` stamped at that block only shadows it for later reads.
	#[test]
	fn destruction_applied_after_extraction_preserves_same_block_transfer() {
		let (resolver, _) = absent_resolver();
		let index = ForeignAssetIndex::new(AssetTransferConfig::default(), resolver);
		let loc = vec![0xCA, 0xFE];

		futures::executor::block_on(async {
			index.record_change(loc.clone(), 1, Some(5)).await; // created earlier

			// Extraction-time read of the transfer in block 10, before the destroy is applied.
			assert_eq!(index.get(&loc, 10).await, Some(5), "resolves at extraction time");

			// Destruction (a later extrinsic in block 10) is applied only now, after extraction.
			index.apply_event("ForeignAssets", "Destroyed", &loc, 10, hash(10)).await;
			assert_eq!(index.get(&loc, 10).await, None, "destruction shadows afterwards");
		});
	}

	#[test]
	fn maintains_every_configured_foreign_instance() {
		let (resolver, _) = fixed_index_resolver(3);
		let config = AssetTransferConfig {
			foreign_instances: vec![("ForeignAssets", 0x0220), ("OtherForeign", 0x0420)],
			..AssetTransferConfig::default()
		};
		let index = ForeignAssetIndex::new(config, resolver);
		let loc = vec![0x55];
		let mut created = loc.clone();
		created.extend_from_slice(&[0u8; 64]);

		futures::executor::block_on(async {
			index.apply_event("OtherForeign", "Created", &created, 10, hash(10)).await;
			assert_eq!(index.get(&loc, 10).await, Some(3), "second foreign instance tracked");
		});
	}

	#[test]
	fn ignores_unrelated_pallets_and_variants() {
		let (resolver, calls) = fixed_index_resolver(1);
		let index = ForeignAssetIndex::new(AssetTransferConfig::default(), resolver);
		let loc = vec![0x07];
		let mut created = loc.clone();
		created.extend_from_slice(&[0u8; 64]);

		futures::executor::block_on(async {
			// `Assets` is a u32-id instance, not a foreign instance -> ignored.
			index.apply_event("Assets", "Created", &created, 10, hash(10)).await;
			// Unrelated variant on a foreign instance -> ignored.
			index.apply_event("ForeignAssets", "Issued", &created, 10, hash(10)).await;
			assert!(index.journal.lock().await.get(&loc).is_none());
		});
		assert_eq!(calls.load(Ordering::SeqCst), 0, "no storage read for ignored events");
	}

	// A pruned/live-mode cutover seed records its mappings stamped at the cutover block: they
	// resolve for any block at or after the cutover, not before, and forward events still
	// supersede.
	#[test]
	fn cutover_seed_is_stamped_at_the_cutover_block() {
		let (resolver, _) = absent_resolver();
		let index = ForeignAssetIndex::new(AssetTransferConfig::default(), resolver);
		let loc = vec![0xAB];

		futures::executor::block_on(async {
			index.seed_at_block(100, [(loc.clone(), 7)]).await;

			assert_eq!(index.get(&loc, 99).await, None, "not resolved before the cutover");
			assert_eq!(index.get(&loc, 100).await, Some(7), "resolved at the cutover");
			assert_eq!(index.get(&loc, 150).await, Some(7), "resolved after the cutover");

			// A forward destruction supersedes the seed for later blocks.
			index.apply_event("ForeignAssets", "Destroyed", &loc, 200, hash(200)).await;
			assert_eq!(index.get(&loc, 150).await, Some(7), "still live before destroy");
			assert_eq!(index.get(&loc, 200).await, None, "destroyed from its block on");
		});
	}

	// ---- event-variant phase classification ----

	// The indexing layer classifies events with `is_foreign_creation` / `is_foreign_destruction` so
	// it can apply creations BEFORE extraction and destructions AFTER. These predicates route each
	// variant to the right phase.
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

	// `disabled()` must be genuinely inert: empty instance lists, so `apply_event` matches nothing
	// even for a real foreign pallet name, and the resolver reports absent so `get` is always None.
	#[test]
	fn disabled_is_truly_inert() {
		let index = ForeignAssetIndex::disabled();
		let loc = vec![0x01];
		let mut created = loc.clone();
		created.extend_from_slice(&[0u8; 64]);

		futures::executor::block_on(async {
			index.apply_event("ForeignAssets", "Created", &created, 10, hash(10)).await;
			assert!(index.journal.lock().await.get(&loc).is_none(), "disabled() records nothing");
			assert_eq!(index.get(&loc, 10).await, None, "disabled() resolves to None");
		});
	}

	// An empty `foreign_instances` config matches no pallet, so even a resolver that WOULD return
	// an index never records one from an event.
	#[test]
	fn empty_foreign_config_matches_nothing() {
		let (resolver, calls) = fixed_index_resolver(7);
		let config = AssetTransferConfig { u32_instances: vec![], foreign_instances: vec![] };
		let index = ForeignAssetIndex::new(config, resolver);
		let loc = vec![0x02];
		let mut created = loc.clone();
		created.extend_from_slice(&[0u8; 64]);

		futures::executor::block_on(async {
			index.apply_event("ForeignAssets", "Created", &created, 10, hash(10)).await;
			assert!(index.journal.lock().await.get(&loc).is_none());
		});
		assert_eq!(calls.load(Ordering::SeqCst), 0, "no resolver call when nothing matches");
	}

	// `resolve_from_storage` reads the mapping directly from chain storage at the block, bypassing
	// the journal (archive-backfill log construction). It never touches or mutates the journal.
	#[test]
	fn resolve_from_storage_reads_at_block_and_bypasses_journal() {
		let (resolver, calls) = fixed_index_resolver(7);
		let index = ForeignAssetIndex::new(AssetTransferConfig::default(), resolver);
		let loc = vec![0xCA, 0xFE];

		futures::executor::block_on(async {
			assert_eq!(index.resolve_from_storage(&loc, hash(50)).await, Some(7), "reads storage");
			assert_eq!(calls.load(Ordering::SeqCst), 1);
			// Each call reads again (no memoization) and the journal stays empty.
			assert_eq!(index.resolve_from_storage(&loc, hash(60)).await, Some(7));
			assert_eq!(calls.load(Ordering::SeqCst), 2, "reads storage on every call");
			assert!(index.journal.lock().await.get(&loc).is_none(), "never writes the journal");
			assert_eq!(index.get(&loc, 50).await, None, "journal lookup is unaffected");
		});
	}

	// An absent or failed storage read resolves to `None`.
	#[test]
	fn resolve_from_storage_absent_is_none() {
		let (resolver, _) = queued_resolver(vec![Ok(None), Err(())]);
		let index = ForeignAssetIndex::new(AssetTransferConfig::default(), resolver);
		let loc = vec![0x07];

		futures::executor::block_on(async {
			assert_eq!(index.resolve_from_storage(&loc, hash(10)).await, None, "absent -> None");
			assert_eq!(
				index.resolve_from_storage(&loc, hash(11)).await,
				None,
				"read failure -> None"
			);
		});
	}

	// A `get` that hits a recorded change is a pure read: repeated hits never alter the journal.
	#[test]
	fn get_hit_is_pure() {
		let (resolver, calls) = absent_resolver();
		let index = ForeignAssetIndex::new(AssetTransferConfig::default(), resolver);
		let loc = vec![1, 2, 3];

		futures::executor::block_on(async {
			index.record_change(loc.clone(), 5, Some(9)).await;
			for _ in 0..5 {
				assert_eq!(index.get(&loc, 10).await, Some(9));
			}
			assert_eq!(calls.load(Ordering::SeqCst), 0, "hits never read storage");
			assert_eq!(
				index.journal.lock().await.get(&loc).map(BTreeMap::len),
				Some(1),
				"unchanged"
			);
		});
	}

	// `forget_blocks` removes journal facts stamped at orphaned-fork heights (and only those), so a
	// reorg can't leave a dead fork's `Created`/`Destroyed` resolving canonical-chain transfers. A
	// `Location` whose every change is forgotten is dropped entirely.
	#[test]
	fn forget_blocks_removes_only_named_heights() {
		let (resolver, _) = absent_resolver();
		let index = ForeignAssetIndex::new(AssetTransferConfig::default(), resolver);
		let loc = vec![0xCA, 0xFE];

		futures::executor::block_on(async {
			index.record_change(loc.clone(), 10, Some(7)).await;
			index.record_change(loc.clone(), 20, None).await;
			index.record_change(loc.clone(), 30, Some(8)).await;

			// Forget only the orphaned height 30; earlier heights are untouched.
			index.forget_blocks(&[30]).await;
			assert_eq!(
				index.get(&loc, 35).await,
				None,
				"height-20 destruction stands after 30 is forgotten"
			);
			assert_eq!(index.get(&loc, 15).await, Some(7), "untouched heights remain");

			// An empty input list is a no-op.
			index.forget_blocks(&[]).await;
			assert_eq!(index.get(&loc, 15).await, Some(7));

			// Forgetting every remaining height drops the location entirely.
			index.forget_blocks(&[10, 20]).await;
			assert!(
				index.journal.lock().await.get(&loc).is_none(),
				"location dropped once it has no remaining changes"
			);
		});
	}

	#[test]
	fn foreign_index_storage_key_layout() {
		let loc = vec![0xDE, 0xAD, 0xBE, 0xEF];
		let key = foreign_index_storage_key(&loc);
		// twox128(pallet) ++ twox128(entry) ++ blake2_128(loc) ++ loc
		assert_eq!(key.len(), 16 + 16 + 16 + loc.len());
		assert_eq!(&key[..16], &twox_128(b"AssetsPrecompiles"));
		assert_eq!(&key[16..32], &twox_128(b"ForeignAssetIdToAssetIndex"));
		assert_eq!(&key[32..48], &blake2_128(&loc));
		assert_eq!(&key[48..], &loc[..]);
	}

	#[test]
	fn location_round_trips_through_storage_key() {
		// The cutover seed enumerates the map by prefix and recovers each `Location` from the full
		// key, so building a key and recovering the location must round-trip.
		let loc = vec![0x01, 0x02, 0x00, 0xCA, 0xFE];
		let key = foreign_index_storage_key(&loc);
		assert_eq!(&key[..32], &foreign_index_prefix()[..]);
		assert_eq!(location_from_foreign_index_key(&key), Some(loc));
		// A key shorter than prefix(32) + hash(16) is not one of this map's entries.
		assert_eq!(location_from_foreign_index_key(&[0u8; 47]), None);
	}

	#[test]
	fn decodes_foreign_index_u32() {
		use codec::Encode;
		let raw = 12345u32.encode();
		assert_eq!(decode_foreign_index(&raw), Some(12345));
	}
}
