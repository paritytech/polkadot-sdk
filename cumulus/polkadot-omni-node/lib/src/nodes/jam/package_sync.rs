// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

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

//! JAM-specific glue over the generic [`jam_package_sync`] protocols.
//!
//! A collator that imports a block authored by another collator cannot recompute that block's
//! work-package hash, because the package contains the author's randomised signature. This module
//! keeps, per imported block, the storage proof the re-execution recorded, so the PoV can be
//! rebuilt locally on demand, and provides the pieces the generic protocol needs from the JAM
//! side:
//!
//! - [`AuraPeerTargets`] orders the collators of a block, author first, self excluded;
//! - [`JamPackageAcceptor`] rebuilds and verifies a peer's package before trusting it.

use super::{
	authorizer::{AuraAuthorizer, AuraPublic},
	ensure_code_blob_at,
	hash_ledger::WpHashLedger,
	package::{build_pov, work_package, work_package_hash, PackageParams},
	scan_pools_at, LOG_TARGET,
};
use crate::common::{
	aura::AuraIdT,
	types::{ParachainBackend, ParachainClient},
	ConstructNodeRuntimeApi, NodeBlock,
};
use async_trait::async_trait;
use cumulus_primitives_core::JamParent;
use futures::channel::mpsc;
use jam_interface::{
	CoreIndex, HeaderHash, JamChainSource, JamStateSource, Slot as JamSlot, WorkPackage,
	WorkPackageHash,
};
use jam_package_sync::{
	fetcher::{AcceptError, PackageInfoAcceptor, PeerTargets},
	store::PackageInfoStore,
	types::PackageInfo,
};
use jam_types::{Authorization, ExtrinsicHash, ExtrinsicSpec, RefineContext};
use sc_client_api::{BlockBackend, HeaderBackend};
use sc_network::PeerId;
use schnellru::{ByLength, LruMap};
use sp_additional_data::AdditionalData;
use sp_api::ProvideRuntimeApi;
use sp_application_crypto::key_types::AUTHORITY_DISCOVERY;
use sp_authority_discovery::AuthorityId;
use sp_consensus_aura::AuraApi;
use sp_crypto_hashing::blake2_256;
use sp_keystore::KeystorePtr;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, NumberFor};
use sp_trie::CompactProof;
use std::{
	marker::PhantomData,
	sync::{Arc, Mutex},
};

/// Capacity of the import-recorded PoV cache, in blocks.
pub(crate) const IMPORTED_POVS_CAPACITY: u32 = 64;

/// The 32 raw bytes of a hash, whatever newtype it is carried in.
fn hash_bytes<H: AsRef<[u8]>>(hash: &H) -> [u8; 32] {
	let slice = hash.as_ref();
	debug_assert_eq!(slice.len(), 32, "hash_bytes is only called with 32-byte block hashes; qed");
	let mut bytes = [0u8; 32];
	bytes.copy_from_slice(slice);
	bytes
}

/// What re-execution recorded for one imported foreign block, so its PoV can be rebuilt.
pub(crate) struct ImportedPov<Block: BlockT> {
	/// The block's parent, whose header the PoV carries.
	pub(crate) parent_hash: Block::Hash,
	/// The block's number, kept for pruning and logging.
	pub(crate) number: NumberFor<Block>,
	/// The PoV spec the rebuild must reproduce.
	pub(crate) spec: ExtrinsicSpec,
	/// The `JamParent` digest the block carries: anchor, lookup anchor and their slots.
	pub(crate) jam_parent: JamParent,
	/// The compact proof recorded during re-execution.
	pub(crate) compact_proof: CompactProof,
	/// The additional-data map the block carried.
	pub(crate) additional_data: AdditionalData,
}

impl<Block: BlockT> Clone for ImportedPov<Block> {
	fn clone(&self) -> Self {
		Self {
			parent_hash: self.parent_hash,
			number: self.number,
			spec: self.spec.clone(),
			jam_parent: self.jam_parent,
			compact_proof: self.compact_proof.clone(),
			additional_data: self.additional_data.clone(),
		}
	}
}

/// The import-recorded PoVs, keyed by the block they belong to, bounded like the package store.
#[derive(Clone)]
pub(crate) struct ImportedPovs<Block: BlockT>(
	Arc<Mutex<LruMap<Block::Hash, ImportedPov<Block>, ByLength>>>,
);

impl<Block: BlockT> ImportedPovs<Block> {
	/// A cache holding at most `capacity` blocks.
	pub(crate) fn new(capacity: u32) -> Self {
		Self(Arc::new(Mutex::new(LruMap::new(ByLength::new(capacity)))))
	}

	fn map(&self) -> std::sync::MutexGuard<'_, LruMap<Block::Hash, ImportedPov<Block>, ByLength>> {
		self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
	}

	/// The recorded entry for `hash`, if any.
	pub(crate) fn get(&self, hash: &Block::Hash) -> Option<ImportedPov<Block>> {
		self.map().get(hash).cloned()
	}

	/// Record `pov` for `hash`, evicting the least recently used entry if full.
	pub(crate) fn insert(&self, hash: Block::Hash, pov: ImportedPov<Block>) {
		self.map().insert(hash, pov);
	}

	/// Rebuild the PoV of `hash` from the recorded proof, the block body in the local database and
	/// its parent header.
	pub(crate) fn rebuild_pov<Client>(
		&self,
		client: &Arc<Client>,
		hash: &Block::Hash,
	) -> Option<Vec<u8>>
	where
		Client: PovRebuildSource<Block>,
	{
		let entry = self.get(hash)?;
		let header = client.header(*hash)?;
		let body = client.block_body(*hash)?;
		let parent_header = client.header(entry.parent_hash)?;
		let block = Block::new(header, body);
		Some(build_pov(&[block], &entry.compact_proof, &parent_header, &entry.additional_data))
	}
}

/// The local-database reads a PoV rebuild needs, implemented for any client that can serve a
/// header and a body.
pub(crate) trait PovRebuildSource<Block: BlockT> {
	/// The header of `hash`, if the local database holds the block.
	fn header(&self, hash: Block::Hash) -> Option<Block::Header>;
	/// The body of `hash`, if the local database holds the block.
	fn block_body(&self, hash: Block::Hash) -> Option<Vec<Block::Extrinsic>>;
}

impl<Block, C> PovRebuildSource<Block> for C
where
	Block: BlockT,
	C: HeaderBackend<Block> + BlockBackend<Block>,
{
	fn header(&self, hash: Block::Hash) -> Option<Block::Header> {
		HeaderBackend::header(self, hash).ok().flatten()
	}

	fn block_body(&self, hash: Block::Hash) -> Option<Vec<Block::Extrinsic>> {
		BlockBackend::block_body(self, hash).ok().flatten()
	}
}

/// A foreign package accepted from a peer and handed to the collation task.
pub(crate) struct ForeignPackage<Block: BlockT> {
	pub(crate) block_hash: Block::Hash,
	pub(crate) block_number: NumberFor<Block>,
	pub(crate) parent_hash: Block::Hash,
	pub(crate) wp_hash: WorkPackageHash,
	pub(crate) anchor_slot: JamSlot,
	/// The rebuilt package, exactly what a resubmission replays.
	pub(crate) package: WorkPackage,
	/// The core whose pool held the para's authorizer at the anchor.
	pub(crate) core: Option<CoreIndex>,
	/// Authority-discovery id of the author, for logging.
	pub(crate) author: Option<AuthorityId>,
}

/// The collators to ask for a block's package info: the block's author first, self excluded.
///
/// The author is the collator the Aura round-robin names for the block's slot, and the aura set
/// and the authority-discovery set are session handlers over the same validators, so their
/// indices align. The local node's AD keys are stripped so it never asks itself.
pub(crate) struct AuraPeerTargets<Block: BlockT, RuntimeApi, AuraId> {
	pub(crate) client: Arc<ParachainClient<Block, RuntimeApi>>,
	pub(crate) keystore: KeystorePtr,
	_marker: PhantomData<(Block, AuraId)>,
}

impl<Block: BlockT, RuntimeApi, AuraId> AuraPeerTargets<Block, RuntimeApi, AuraId> {
	pub(crate) fn new(
		client: Arc<ParachainClient<Block, RuntimeApi>>,
		keystore: KeystorePtr,
	) -> Self {
		Self { client, keystore, _marker: PhantomData }
	}
}

impl<Block, RuntimeApi, AuraId> PeerTargets<Block> for AuraPeerTargets<Block, RuntimeApi, AuraId>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraApi<Block, AuraPublic<AuraId>>,
	AuraId: AuraIdT,
{
	fn targets(&self, header: &Block::Header) -> Vec<AuthorityId> {
		let parent = *header.parent_hash();
		let slot = match sc_consensus_aura::find_pre_digest::<
			Block,
			<AuraId as AuraIdT>::BoundedSignature,
		>(header)
		{
			Ok(slot) => *slot,
			Err(error) => {
				tracing::debug!(
					target: LOG_TARGET,
					?parent,
					%error,
					"No Aura pre-digest in the header, so its author is unknown; not asking anyone.",
				);
				return Vec::new();
			},
		};

		let api = self.client.runtime_api();
		let aura = match <RuntimeApi::RuntimeApi as AuraApi<Block, AuraPublic<AuraId>>>::authorities(
			&*api, parent,
		) {
			Ok(authorities) => authorities,
			Err(error) => {
				tracing::debug!(
					target: LOG_TARGET,
					?parent,
					%error,
					"Unable to read the aura authorities; not asking anyone.",
				);
				return Vec::new();
			},
		};
		let ad = match <RuntimeApi::RuntimeApi as sp_authority_discovery::AuthorityDiscoveryApi<
			Block,
		>>::authorities(&*api, parent)
		{
			Ok(authorities) => authorities,
			Err(error) => {
				tracing::debug!(
					target: LOG_TARGET,
					?parent,
					%error,
					"Unable to read the authority-discovery authorities; not asking anyone.",
				);
				return Vec::new();
			},
		};

		let local = self.keystore.keys(AUTHORITY_DISCOVERY).unwrap_or_default();
		order_targets(slot, aura.len(), ad, &local)
	}
}

/// Order the authority-discovery ids of a block: its author first, then the rest, minus our own.
///
/// The author is `ad[slot % aura.len()]`. When the aura and AD sets disagree in length the two
/// sets are not the aligned session handlers the protocol assumes, so the AD list is handed back
/// as-is (self still excluded) rather than guessing an author.
fn order_targets(
	slot: u64,
	aura_len: usize,
	ad: Vec<AuthorityId>,
	local: &[Vec<u8>],
) -> Vec<AuthorityId> {
	if aura_len != ad.len() {
		tracing::warn!(
			target: LOG_TARGET,
			slot,
			aura_len,
			authority_discovery_len = ad.len(),
			"The aura and authority-discovery sets differ in length; asking the AD set unordered.",
		);
		let mut unordered = ad;
		unordered.retain(|id| !is_local(id, local));
		return unordered;
	}
	if ad.is_empty() {
		return Vec::new();
	}

	let index = (slot % ad.len() as u64) as usize;
	let mut ordered = Vec::with_capacity(ad.len());
	ordered.push(ad[index].clone());
	ordered.extend(ad.into_iter().enumerate().filter(|(i, _)| *i != index).map(|(_, id)| id));
	ordered.retain(|id| !is_local(id, local));
	ordered
}

/// Whether `id` is one of the local authority-discovery keys.
fn is_local(id: &AuthorityId, local: &[Vec<u8>]) -> bool {
	let bytes: &[u8] = id.as_ref();
	local.iter().any(|key| key.as_slice() == bytes)
}

/// Rebuilds and verifies a work package from a peer's [`PackageInfo`], then hands it to the
/// collation task.
pub(crate) struct JamPackageAcceptor<Block: BlockT, RuntimeApi, Jam> {
	pub(crate) para_client: Arc<ParachainClient<Block, RuntimeApi>>,
	pub(crate) para_backend: Arc<ParachainBackend<Block>>,
	pub(crate) jam: Arc<Jam>,
	pub(crate) params: PackageParams,
	pub(crate) authorizer: Arc<AuraAuthorizer>,
	pub(crate) imported: ImportedPovs<Block>,
	pub(crate) store: Arc<PackageInfoStore<Block::Hash>>,
	pub(crate) ledger: WpHashLedger<ParachainClient<Block, RuntimeApi>>,
	pub(crate) foreign_tx: mpsc::Sender<ForeignPackage<Block>>,
}

#[async_trait]
impl<Block, RuntimeApi, Jam> PackageInfoAcceptor<Block>
	for JamPackageAcceptor<Block, RuntimeApi, Jam>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	Jam: JamChainSource + JamStateSource + Send + Sync + 'static,
{
	fn is_known(&self, block: &Block::Hash) -> bool {
		if self.store.contains(block) {
			return true;
		}
		let block_bytes = hash_bytes(block);
		self.ledger.get(&block_bytes).ok().flatten().is_some()
	}

	async fn accept(
		&self,
		header: &Block::Header,
		info: PackageInfo,
		_from: PeerId,
	) -> Result<(), AcceptError> {
		let block_hash = header.hash();
		let parent_hash = *header.parent_hash();

		// The PoV can only be rebuilt from the proof re-execution recorded at import. A block
		// without one cannot be resubmitted, so its package is unusable.
		let Some(imported) = self.imported.get(&block_hash) else {
			return Err(AcceptError::Unusable(format!(
				"no recorded proof for block {block_hash:?}, so the package cannot be rebuilt"
			)));
		};
		let JamParent { anchor, anchor_slot, lookup_anchor, lookup_anchor_slot } =
			imported.jam_parent;
		let anchor = HeaderHash::from(hash_bytes(&anchor));
		let lookup_anchor = HeaderHash::from(hash_bytes(&lookup_anchor));

		ensure_code_blob_at(
			&self.para_client,
			&self.para_backend,
			&*self.jam,
			self.params.service_id,
			parent_hash,
			anchor,
		)
		.await;

		let validation_code = self.para_client.code_at(parent_hash).map_err(|error| {
			AcceptError::Rejected(format!("cannot read the validation code: {error}"))
		})?;
		let validation_code_hash = blake2_256(&validation_code);

		let state_root =
			self.jam.state_root(anchor).await.map_err(|error| {
				AcceptError::Rejected(format!("state root at the anchor: {error}"))
			})?;
		let beefy_root =
			self.jam.beefy_root(anchor).await.map_err(|error| {
				AcceptError::Rejected(format!("beefy root at the anchor: {error}"))
			})?;
		let lookup_anchor_state_root =
			self.jam.state_root(lookup_anchor).await.map_err(|error| {
				AcceptError::Rejected(format!("state root at the lookup anchor: {error}"))
			})?;

		let author_spec = ExtrinsicSpec { hash: ExtrinsicHash(info.pov.hash), len: info.pov.len };

		let context = RefineContext {
			anchor,
			anchor_slot,
			state_root,
			beefy_root,
			lookup_anchor,
			lookup_anchor_slot,
			lookup_anchor_state_root,
			prerequisites: info
				.prerequisites
				.iter()
				.map(|hash| WorkPackageHash(*hash))
				.collect::<Vec<_>>()
				.into(),
		};

		let mut package = work_package(
			author_spec,
			validation_code_hash,
			&self.params,
			self.authorizer.authorizer(),
			context,
		);
		package.authorization = Authorization(info.authorization.clone());

		self.authorizer.verify_token(&package).map_err(|error| {
			AcceptError::Rejected(format!("the token does not verify: {error}"))
		})?;

		let wp_hash = work_package_hash(&package);
		let scan = scan_pools_at(&*self.jam, anchor, &self.authorizer)
			.await
			.map_err(AcceptError::Rejected)?;
		let core = scan.target;

		self.store.insert(block_hash, info);
		let block_bytes = hash_bytes(&block_hash);
		if let Err(error) = self.ledger.insert(&block_bytes, wp_hash) {
			tracing::warn!(
				target: LOG_TARGET,
				?block_hash,
				?wp_hash,
				%error,
				"Unable to record the verified foreign work-package hash; a child of this block \
				 starts a new chain.",
			);
		}

		let author = self.author_of(parent_hash, lookup_anchor_slot);
		let block_number = *header.number();
		let mut foreign_tx = self.foreign_tx.clone();
		if foreign_tx
			.try_send(ForeignPackage {
				block_hash,
				block_number,
				parent_hash,
				wp_hash,
				anchor_slot,
				package,
				core,
				author: author.clone(),
			})
			.is_err()
		{
			tracing::debug!(
				target: LOG_TARGET,
				?block_hash,
				?wp_hash,
				"The collation task's foreign-package channel is full or gone; the package is \
				 stored but will not be resubmitted by this collator.",
			);
		}

		tracing::info!(
			target: LOG_TARGET,
			?block_hash,
			%block_number,
			?parent_hash,
			?wp_hash,
			anchor_slot,
			lookup_anchor_slot,
			?core,
			?author,
			"Verified another collator's work package.",
		);
		Ok(())
	}
}

impl<Block, RuntimeApi, Jam> JamPackageAcceptor<Block, RuntimeApi, Jam>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
{
	/// The authority-discovery id of the collator the Aura round-robin names for
	/// `lookup_anchor_slot`, when the AD set is readable and aligned.
	fn author_of(
		&self,
		parent_hash: Block::Hash,
		lookup_anchor_slot: JamSlot,
	) -> Option<AuthorityId> {
		let api = self.para_client.runtime_api();
		let ad = <RuntimeApi::RuntimeApi as sp_authority_discovery::AuthorityDiscoveryApi<
			Block,
		>>::authorities(&*api, parent_hash)
		.ok()?;
		if ad.is_empty() {
			return None;
		}
		let index = self.authorizer.collator_for(lookup_anchor_slot) as usize % ad.len();
		ad.get(index).cloned()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::nodes::jam::package::{extrinsic_spec, pov_spec};
	use cumulus_jam_state_reader::JAM_PROOF_KEY;
	use cumulus_primitives_core::JamParent;
	use cumulus_test_runtime::{Block as TestBlock, Header as TestHeader};
	use sp_core::{crypto::ByteArray, H256};

	fn id(byte: u8) -> AuthorityId {
		AuthorityId::from_slice(&[byte; 32]).expect("32 bytes is a valid authority id; qed")
	}

	fn local_key(id: &AuthorityId) -> Vec<u8> {
		id.as_slice().to_vec()
	}

	/// The block's author is asked first, then the rest of the set, and this node never asks
	/// itself.
	#[test]
	fn targets_are_author_first_and_self_excluded() {
		let ad = vec![id(0), id(1), id(2)];

		// Slot 1 names authority 1, which is the author.
		let ordered = order_targets(1, 3, ad.clone(), &[]);
		assert_eq!(
			ordered.iter().map(|id| id.as_slice()).collect::<Vec<_>>(),
			vec![id(1).as_slice(), id(0).as_slice(), id(2).as_slice()],
			"the author comes first and the rest keep their order",
		);

		let without_self = order_targets(1, 3, ad.clone(), &[local_key(&id(0))]);
		assert!(
			!without_self.iter().any(|candidate| candidate.as_slice() == id(0).as_slice()),
			"this node's own authority is never asked",
		);

		assert!(order_targets(0, 0, Vec::new(), &[]).is_empty(), "no AD set, no targets");
	}

	/// When the aura and AD sets disagree the pair is not the aligned session handlers the
	/// protocol assumes, so the AD set is handed back unordered with self still excluded.
	#[test]
	fn mismatched_sets_are_returned_unordered_and_self_excluded() {
		let ordered = order_targets(1, 2, vec![id(0), id(1), id(2)], &[local_key(&id(1))]);

		assert_eq!(
			ordered.iter().map(|id| id.as_slice()).collect::<Vec<_>>(),
			vec![id(0).as_slice(), id(2).as_slice()],
		);
	}

	struct MockPovClient {
		header: TestHeader,
		parent: TestHeader,
		body: Vec<<TestBlock as BlockT>::Extrinsic>,
	}

	impl PovRebuildSource<TestBlock> for MockPovClient {
		fn header(&self, hash: <TestBlock as BlockT>::Hash) -> Option<TestHeader> {
			if hash == self.header.hash() {
				Some(self.header.clone())
			} else if hash == self.parent.hash() {
				Some(self.parent.clone())
			} else {
				None
			}
		}

		fn block_body(
			&self,
			hash: <TestBlock as BlockT>::Hash,
		) -> Option<Vec<<TestBlock as BlockT>::Extrinsic>> {
			(hash == self.header.hash()).then(|| self.body.clone())
		}
	}

	/// The proof re-execution recorded has to rebuild the very PoV spec it stored, so a resend can
	/// replay the bytes the author's package commits to.
	#[test]
	fn rebuild_pov_reproduces_the_recorded_spec() {
		let parent = TestHeader::new(
			0,
			H256::repeat_byte(1),
			H256::repeat_byte(2),
			H256::zero(),
			Default::default(),
		);
		let header = TestHeader::new(
			1,
			H256::repeat_byte(1),
			H256::repeat_byte(2),
			parent.hash(),
			Default::default(),
		);
		let hash = header.hash();
		let proof = CompactProof { encoded_nodes: vec![vec![1u8, 2, 3]] };
		let additional_data: AdditionalData = [(JAM_PROOF_KEY.to_string(), vec![4u8, 5, 6])].into();
		let block = TestBlock::new(header.clone(), Vec::new());
		let spec = pov_spec(&[block], &proof, &parent, &additional_data);

		let imported = ImportedPovs::new(4);
		imported.insert(
			hash,
			ImportedPov {
				parent_hash: parent.hash(),
				number: 1,
				spec: spec.clone(),
				jam_parent: JamParent {
					anchor: H256::repeat_byte(9),
					anchor_slot: 0,
					lookup_anchor: H256::repeat_byte(9),
					lookup_anchor_slot: 0,
				},
				compact_proof: proof,
				additional_data,
			},
		);

		let client = Arc::new(MockPovClient { header, parent, body: Vec::new() });
		let rebuilt = imported.rebuild_pov(&client, &hash).expect("the recorded proof rebuilds");
		let rebuilt_spec = extrinsic_spec(&rebuilt);

		assert_eq!(rebuilt_spec.hash, spec.hash, "the rebuilt PoV has the recorded hash");
		assert_eq!(rebuilt_spec.len, spec.len, "and the recorded length");
	}
}
