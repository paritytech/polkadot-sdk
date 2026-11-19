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

//! The JAM collator (phase-1 PoC): a parachain node whose backing chain is JAM instead of a
//! relay chain.
//!
//! Two tasks form the collator loop:
//!
//! - the [builder task](builder_task) authors on a wall-clock parachain-slot timer, anchoring at
//!   the JAM tip it caches, building on the deepest block the local database holds below the head
//!   JAM has accumulated rather than waiting for inclusion — or on a sibling of that head once it
//!   has stood still long enough to say the branch above it is lost — with the shared authoring
//!   primitives and a *mocked* parachain inherent, one block per core that holds the para's
//!   authorizer, and feeds the channel. A block's timestamp is its anchor's and its Aura slot is
//!   the anchor's Aura slot, so the collator cannot pick its own time; the authorizer that signs a
//!   package is the one JAM uses at the package's anchor slot, and a turn stops at a block that
//!   changes the Aura set;
//! - the [collation task](collation_task) turns each block into a work package — naming the work
//!   package this node submitted for the parent block as its one prerequisite when the ledger still
//!   remembers it — submits it, follows `workPackageStatus` for every package in flight, and
//!   re-decides every package at each JAM block behind a pluggable [policy](resubmission);
//!
//! The collation task writes the [authoring hold](AuthoringHold) and the builder reads it each
//! tick: while it is non-zero the builder stands down, because a new block would only deepen a
//! branch whose package has not landed yet.
//!
//! # The PoV store
//!
//! A work package carries its author's randomised signature, so it cannot be recomputed, not even
//! by its author: a block whose package was lost can only travel again in a new package. That
//! takes the block's PoV, and so the storage proof the block executes with. The
//! [PoV store](pov_store::BlockPovs) keeps it per block: the import path records it for every
//! block it re-executes, the collation task for every block this collator authored.
//!
//! Heads following reuses `cumulus_client_consensus_common::run_parachain_consensus` with
//! streams built from the parachain service's per-para state entry: `ParaInfo.head_data` holds
//! the head's header hash, which is resolved to the header from the local database.

pub(crate) mod block_import;
pub(crate) mod builder_task;
pub(crate) mod collation_task;
pub(crate) mod hash_ledger;
pub(crate) mod package;
pub(crate) mod pov_store;
pub(crate) mod resubmission;

pub(crate) use cumulus_client_jam_authorizer::authorizer;

use crate::common::{
	aura::{AuraIdT, AuraRuntimeApi},
	types::{ParachainBackend, ParachainClient},
	ConstructNodeRuntimeApi, NodeBlock,
};
use authorizer::AuraAuthorizer;
use codec::{Decode, DecodeAll, Encode};
use cumulus_jam_aura_primitives::JamAuraApi;
use cumulus_jam_state_reader::{JamStateExt, JamStateReader};
use futures::{Stream, StreamExt};
use jam_interface::{
	AuthPool, AuthorizerHash, BlockDesc, CoreIndex, HeaderHash, JamStateSource, ServiceId,
	Slot as JamSlot,
};
use jam_types::RefineContext;
use parachain_service_core::{para_info_key, ParaInfo};
use sc_client_api::{Backend as _, BlockchainEvents, StorageProvider};
use sp_additional_data::AdditionalData;
use sp_api::{ApiExt, CallApiAt, CallApiAtParams, CallContext, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_consensus::ProposeArgs;
use sp_consensus_aura::AuraApi;
use sp_core::Pair;
use sp_keystore::KeystorePtr;
use sp_runtime::traits::Block as BlockT;
use sp_timestamp::Timestamp;
use std::{
	collections::HashMap,
	fmt::Debug,
	future::Future,
	marker::PhantomData,
	path::PathBuf,
	pin::Pin,
	sync::{
		atomic::{AtomicUsize, Ordering},
		Arc, Mutex,
	},
	time::Instant,
};

pub(crate) const LOG_TARGET: &str = "jam-collator";

pub(crate) const JAM_SLOT_DURATION_MS: u64 = 6000;

/// The public key of an aura authority, as `AuraApi::authorities()` hands it out.
pub(crate) type AuraPublic<AuraId> = <<AuraId as AuraIdT>::BoundedPair as Pair>::Public;

/// Where the collator gets the para's AURA authorizer for an anchor slot.
///
/// JAM authorizes packages with the set it uses at the package's *anchor* slot, and a set change
/// is announced a session ahead. The runtime knows the schedule, so every decision that depends
/// on who may author — signing, the round-robin turn, the pool scan — asks this source for the
/// anchor's set rather than caching one set for the node's life.
pub(crate) trait AuthoritySource<Block: BlockT>: Send + Sync {
	/// The authorizer JAM uses at `anchor_slot`, read from the runtime state at `at`.
	///
	/// `Ok(None)` means this node holds no aura key of that set: it may not author there. `Err`
	/// means the set could not be read or derived, which is a configuration or runtime problem
	/// rather than a turn that simply is not ours.
	fn for_anchor(
		&self,
		at: Block::Hash,
		anchor_slot: JamSlot,
	) -> Result<Option<Arc<AuraAuthorizer>>, String>;
}

/// The authority sets a runtime names, by anchor slot.
///
/// Split out from [`Authorizers`] so the slot-bound selection can be tested without a client.
pub(crate) trait RuntimeAuthorities<Block: BlockT, AuraId: AuraIdT>: Send + Sync {
	/// The set the runtime names for a package anchored at `anchor_slot`, read at `at`.
	fn authorities_at(
		&self,
		at: Block::Hash,
		anchor_slot: JamSlot,
	) -> Result<Vec<AuraPublic<AuraId>>, String>;
}

/// [`RuntimeAuthorities`] backed by the parachain client's runtime API.
pub(crate) struct RuntimeApiAuthorities<Block: BlockT, RuntimeApi, AuraId> {
	client: Arc<ParachainClient<Block, RuntimeApi>>,
	_phantom: PhantomData<AuraId>,
}

impl<Block, RuntimeApi, AuraId> RuntimeApiAuthorities<Block, RuntimeApi, AuraId>
where
	Block: NodeBlock,
{
	pub(crate) fn new(client: Arc<ParachainClient<Block, RuntimeApi>>) -> Self {
		Self { client, _phantom: PhantomData }
	}
}

impl<Block, RuntimeApi, AuraId> RuntimeAuthorities<Block, AuraId>
	for RuntimeApiAuthorities<Block, RuntimeApi, AuraId>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
{
	fn authorities_at(
		&self,
		at: Block::Hash,
		anchor_slot: JamSlot,
	) -> Result<Vec<AuraPublic<AuraId>>, String> {
		let has_jam_aura_api = self
			.client
			.runtime_api()
			.has_api::<dyn JamAuraApi<Block, AuraPublic<AuraId>>>(at)
			.unwrap_or(false);
		if !has_jam_aura_api {
			// A runtime without the slot-bound API has one set for all slots; falling back keeps
			// coretime-westend and the test runtime authoring exactly as before.
			return self
				.client
				.runtime_api()
				.authorities(at)
				.map_err(|error| format!("AuraApi::authorities at {at:?}: {error}"));
		}

		// A direct call with the method name the runtime dispatch table uses; the arguments are
		// the tuple `(anchor_slot,)` encoded exactly as the generated client method encodes them.
		// Calling through `CallApiAt` rather than a trait bound on the node's `RuntimeApi` keeps
		// every runtime the omni-node serves compiling without implementing the new API.
		let raw = self
			.client
			.call_api_at(CallApiAtParams {
				at,
				function: "JamAuraApi_authorities_at",
				arguments: anchor_slot.encode(),
				overlayed_changes: &Default::default(),
				call_context: CallContext::Offchain,
				recorder: &None,
				extensions: &Default::default(),
			})
			.map_err(|error| {
				format!("JamAuraApi::authorities_at({anchor_slot}) at {at:?}: {error}")
			})?;
		Vec::<AuraPublic<AuraId>>::decode(&mut &raw[..]).map_err(|error| {
			format!(
				"JamAuraApi::authorities_at({anchor_slot}) at {at:?} returned undecodable data: {error}"
			)
		})
	}
}

/// Derives and caches the para's authorizers, one per authority set.
///
/// The set comes from the runtime's `JamAuraApi::authorities_at` when it exposes it, and from
/// `AuraApi::authorities` otherwise — a runtime without the slot-bound API only has one set, and
/// falling back keeps it authoring exactly as before. A derived `AuraAuthorizer` is cached by the
/// exact authority list it commits to, so a set change costs one derivation and a node whose key
/// is absent from a set keeps `None` for that set instead of failing every tick.
pub(crate) struct Authorizers<Block: BlockT, AuraId: AuraIdT> {
	source: Arc<dyn RuntimeAuthorities<Block, AuraId>>,
	blob_path: PathBuf,
	para_id: u32,
	service_id: ServiceId,
	slot_duration: u32,
	keystore: KeystorePtr,
	/// Keyed by the SCALE-encoded authority list, which is exactly what the authorizer commits to.
	cache: Mutex<HashMap<Vec<u8>, Option<Arc<AuraAuthorizer>>>>,
}

impl<Block: BlockT, AuraId: AuraIdT> Authorizers<Block, AuraId> {
	pub(crate) fn new(
		source: Arc<dyn RuntimeAuthorities<Block, AuraId>>,
		blob_path: PathBuf,
		para_id: u32,
		service_id: ServiceId,
		slot_duration: u32,
		keystore: KeystorePtr,
	) -> Self {
		Self {
			source,
			blob_path,
			para_id,
			service_id,
			slot_duration,
			keystore,
			cache: Mutex::new(HashMap::new()),
		}
	}
}

impl<Block: BlockT, AuraId: AuraIdT> AuthoritySource<Block> for Authorizers<Block, AuraId> {
	fn for_anchor(
		&self,
		at: Block::Hash,
		anchor_slot: JamSlot,
	) -> Result<Option<Arc<AuraAuthorizer>>, String> {
		let authorities = self.source.authorities_at(at, anchor_slot)?;
		let key = authorities.encode();
		if let Some(cached) =
			self.cache.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(&key)
		{
			return Ok(cached.clone());
		}

		let authorizer = AuraAuthorizer::try_new::<AuraId>(
			&authorities,
			&self.blob_path,
			self.para_id,
			self.service_id,
			self.slot_duration,
			self.keystore.clone(),
		)?
		.map(Arc::new);
		tracing::info!(
			target: LOG_TARGET,
			?at,
			anchor_slot,
			collators = authorities.len(),
			ours = authorizer.as_ref().map(|authorizer| authorizer.own_index()),
			"Resolved the aura authorizer for an anchor slot.",
		);
		self.cache
			.lock()
			.unwrap_or_else(|poisoned| poisoned.into_inner())
			.insert(key, authorizer.clone());
		Ok(authorizer)
	}
}

/// How many in-flight resubmission packages the builder has to wait for before it authors again.
/// Written by the collation task, read by the builder at tick time.
#[derive(Clone)]
pub(crate) struct AuthoringHold(Arc<AtomicUsize>);

impl AuthoringHold {
	pub(crate) fn new() -> Self {
		Self(Arc::new(AtomicUsize::new(0)))
	}

	pub(crate) fn set_pending(&self, n: usize) {
		self.0.store(n, Ordering::Relaxed);
	}

	pub(crate) fn pending(&self) -> usize {
		self.0.load(Ordering::Relaxed)
	}
}

/// Cache the runtime code a block's `:code` marker names, fetching the preimage from JAM when it
/// is not already in the `CODE` column.
///
/// The runtime writes the marker during block execution, so the block that executes the upgrade
/// holds it and every later read of that block's runtime (`code_at`, `slot_duration_at`, a runtime
/// API) needs the bytes before the node can resolve the marker. A no-op for a block whose `:code`
/// is real code, and for a marker whose blob is already cached.
pub(crate) async fn ensure_code_blob_at<Block, RuntimeApi, Jam>(
	para_client: &Arc<ParachainClient<Block, RuntimeApi>>,
	para_backend: &ParachainBackend<Block>,
	jam: &Jam,
	service_id: ServiceId,
	block: Block::Hash,
	anchor: HeaderHash,
) where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	Jam: JamStateSource,
{
	let key = sp_core::storage::StorageKey(sp_core::storage::well_known_keys::CODE.to_vec());
	let code = match para_client.storage(block, &key) {
		Ok(Some(code)) => code.0,
		Ok(None) => return,
		Err(error) => {
			tracing::warn!(target: LOG_TARGET, ?block, ?error, "Unable to read `:code`.");
			return;
		},
	};
	let Some(code_hash) = sp_code_marker::decode_marker(&code) else {
		return;
	};
	match para_backend.code_blob(&code_hash) {
		Ok(Some(_)) => return,
		Ok(None) => {},
		Err(error) => {
			tracing::warn!(
				target: LOG_TARGET,
				?code_hash,
				?error,
				"Unable to read the local runtime code blob.",
			);
			return;
		},
	}
	match jam.service_preimage(anchor, service_id, code_hash).await {
		Ok(Some(preimage)) => {
			tracing::info!(
				target: LOG_TARGET,
				?code_hash,
				"Fetched the offchain runtime code from JAM.",
			);
			if let Err(error) = para_backend.store_code_blob(&code_hash, &preimage) {
				tracing::error!(
					target: LOG_TARGET,
					?code_hash,
					?error,
					"Unable to cache the fetched runtime code; the block will be dropped.",
				);
			}
		},
		Ok(None) => {
			tracing::error!(
				target: LOG_TARGET,
				?code_hash,
				"JAM holds no preimage for the runtime code marker; the block will be dropped.",
			);
		},
		Err(error) => {
			tracing::error!(
				target: LOG_TARGET,
				?code_hash,
				?error,
				"Failed to fetch the runtime code preimage from JAM; the block will be dropped.",
			);
		},
	}
}

/// Message from the builder task to the collation task: one built parachain block plus the JAM
/// context it was built against.
pub(crate) struct JamCollatorMessage<Block: BlockT> {
	pub parent_header: Block::Header,
	pub block: Block,
	pub proof: sp_api::StorageProof,
	/// The refine context captured at build time; the anchor inside it decides the submission
	/// window.
	pub context: RefineContext,
	/// The timeslot of the anchor block, which starts the ~8-block clock the package has to be
	/// reported within. The collation task needs it to tell an expired anchor apart from any
	/// other reason a package failed.
	pub anchor_slot: JamSlot,
	/// The core whose authorizer pool held this para's authorizer at the anchor, if any. `None`
	/// means the package must not be submitted anywhere — no guarantor would authorize it.
	pub submit_target: Option<CoreIndex>,
	/// The additional-data map to carry in the PoV: the `JAM_PROOF_KEY` entry holding the
	/// SCALE-encoding of `(anchor_state_root, lookup_state_root, proof)` for the para head at
	/// the anchor.
	pub additional_data: AdditionalData,
	/// The JAM best block that triggered this build (for logging).
	pub triggered_by: BlockDesc,
}

/// A JAM-state reader that proves every key absent: the only truthful answer for a node with no
/// JAM state behind it. `jam_state_read` then returns `-1` and the runtime falls back to the
/// relay path instead of trapping on a missing [`JamStateExt`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct NoJamStateReader;

impl JamStateReader for NoJamStateReader {
	fn read(&self, _key: &cumulus_jam_state_reader::StateKey) -> Option<Vec<u8>> {
		None
	}

	fn proof_size(&self) -> usize {
		0
	}
}

/// Wraps a block-authoring [`sp_consensus::Proposer`] so every proposal runs with a proven-absent
/// JAM state reader registered — the omni-node dev (manual/instant-seal) path authors the same
/// polkavm runtime the JAM collator does, but holds no JAM state to read from.
pub(crate) struct JamStateInjectingProposer<P> {
	inner: P,
}

impl<B, P> sp_consensus::Proposer<B> for JamStateInjectingProposer<P>
where
	B: BlockT,
	P: sp_consensus::Proposer<B>,
{
	type Error = P::Error;
	type Proposal = P::Proposal;

	fn propose(self, mut args: ProposeArgs<B>) -> Self::Proposal {
		args.extra_extensions.register(JamStateExt(Box::new(NoJamStateReader)));
		self.inner.propose(args)
	}
}

/// Wraps a block-authoring [`sp_consensus::Environment`] so every proposer it yields injects
/// [`JamStateExt`].
pub(crate) struct JamStateInjectingEnv<E> {
	inner: E,
}

impl<E> JamStateInjectingEnv<E> {
	pub(crate) fn new(inner: E) -> Self {
		Self { inner }
	}
}

impl<B, E> sp_consensus::Environment<B> for JamStateInjectingEnv<E>
where
	B: BlockT,
	E: sp_consensus::Environment<B>,
{
	type CreateProposer = Pin<Box<dyn Future<Output = Result<Self::Proposer, Self::Error>> + Send>>;
	type Proposer = JamStateInjectingProposer<E::Proposer>;
	type Error = E::Error;

	fn init(&mut self, parent_header: &B::Header) -> Self::CreateProposer {
		let init = self.inner.init(parent_header);
		Box::pin(async move {
			let proposer = init.await?;
			Ok(JamStateInjectingProposer { inner: proposer })
		})
	}
}

/// The wall-clock timestamp of a JAM timeslot: slots are 6 s, counted from the JAM common era.
///
/// This makes the parachain timestamp a pure function of the JAM anchor — deterministic under
/// import re-execution, and pre-aligned with the phase-4 time-base remap.
pub(crate) fn jam_slot_timestamp(slot: JamSlot) -> Timestamp {
	Timestamp::new((jam_types::JAM_COMMON_ERA + u64::from(slot) * 6) * 1000)
}

/// The JAM timeslot a wall-clock timestamp falls into — the inverse of [`jam_slot_timestamp`].
///
/// The builder is driven by the wall clock, so it has to know which JAM slot it is in: that is
/// what tells it whether the JAM tip it caches is current, and it is the slot the mocked
/// parachain inherent advertises.
pub(crate) fn jam_slot_at(timestamp: Timestamp) -> JamSlot {
	(timestamp.as_millis().saturating_sub(jam_types::JAM_COMMON_ERA * 1000) / JAM_SLOT_DURATION_MS)
		as JamSlot
}

/// The fake relay slot the mocked parachain inherent advertises for a JAM timeslot (both are
/// 6 s, so this is the timestamp expressed in relay slots).
pub(crate) fn jam_slot_as_relay_slot(slot: JamSlot) -> u64 {
	jam_slot_timestamp(slot).as_millis() / JAM_SLOT_DURATION_MS
}

/// How old a lookup anchor may be when a guarantor sees it.
///
/// polkajam refuses to guarantee a package whose lookup anchor is older than this
/// (`max_lookup_anchor_age`, 24 slots on tiny networks), on top of requiring it to be finalized
/// and its `lookup_anchor_slot` to be exactly that block's slot.
const MAX_LOOKUP_ANCHOR_AGE: JamSlot = 24;
/// How much of that age is reserved for the package still being in flight.
///
/// A package is submitted at its anchor and has [`REPORT_DEADLINE_SLOTS`](resubmission)
/// slots to be reported, so a lookup anchor picked this close to the limit would expire in flight
/// — and a package that dies for that reason looks exactly like one that was never submitted.
const LOOKUP_ANCHOR_SAFETY_MARGIN: JamSlot = 8;

/// Whether a lookup anchor this old would still be inside polkajam's window when the package
/// carrying it is reported, rather than expiring in flight.
pub(crate) fn lookup_anchor_survives_reporting(
	anchor_slot: JamSlot,
	lookup_anchor_slot: JamSlot,
) -> bool {
	anchor_slot.saturating_sub(lookup_anchor_slot) <=
		MAX_LOOKUP_ANCHOR_AGE.saturating_sub(LOOKUP_ANCHOR_SAFETY_MARGIN)
}

/// The lookup anchor a package carries: the newest finalized block, as long as it survives the
/// package's time in flight.
///
/// Under slot-bound sets the *anchor* slot selects the collator and the set is announced a
/// session ahead, so the round-robin walk this replaces is gone; what remains is polkajam's
/// freshness rule. `None` means the tick has to be skipped — a package anchored now would carry
/// a lookup anchor that expires before it could be reported.
pub(crate) fn choose_lookup_anchor(anchor: &BlockDesc, newest: BlockDesc) -> Option<BlockDesc> {
	let age = anchor.slot.saturating_sub(newest.slot);
	if !lookup_anchor_survives_reporting(anchor.slot, newest.slot) {
		tracing::warn!(
			target: LOG_TARGET,
			anchor = ?anchor.header_hash,
			anchor_slot = anchor.slot,
			lookup_anchor = ?newest.header_hash,
			lookup_anchor_slot = newest.slot,
			age,
			max_age = MAX_LOOKUP_ANCHOR_AGE,
			safety_margin = LOOKUP_ANCHOR_SAFETY_MARGIN,
			"The newest finalized block would expire before the package carrying it could be
			 reported; skipping this tick.",
		);
		return None;
	}

	tracing::debug!(
		target: LOG_TARGET,
		anchor = ?anchor.header_hash,
		anchor_slot = anchor.slot,
		lookup_anchor = ?newest.header_hash,
		lookup_anchor_slot = newest.slot,
		age,
		"Chose the newest finalized block as the lookup anchor.",
	);
	Some(newest)
}

/// Which cores currently hold this para's authorizer, and therefore where its packages may go.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PoolScan {
	/// The core the first package of a turn goes to. `None` means no core holds the para's
	/// authorizer at all — the builder keeps authoring, but nothing may be submitted.
	pub target: Option<CoreIndex>,
	/// The cores the following packages of the same turn go to, in the order the scan found
	/// them. A turn puts one package per core, so its packages land on different cores of the
	/// same JAM block — and a core that silently held our packages would otherwise be invisible.
	pub also_on: Vec<CoreIndex>,
}

impl PoolScan {
	/// The core the `package`-th package of a turn goes to: the lowest-indexed core first, then
	/// the others in the order the scan found them. `None` means the turn has no core left.
	///
	/// One package per core is what makes a turn elastic: each package lands on a core of its
	/// own, all in the same JAM block, and every package after the first chains on the previous
	/// one's block.
	pub(crate) fn core_for_package(&self, package: usize) -> Option<CoreIndex> {
		match package {
			0 => self.target,
			_ => self.also_on.get(package - 1).copied(),
		}
	}

	/// How many cores hold this para's authorizer, and therefore how many packages one turn may
	/// put on different cores.
	pub(crate) fn core_count(&self) -> usize {
		self.target.map_or(0, |_| 1 + self.also_on.len())
	}
}

/// Find the cores whose authorizer pool holds `wanted`, lowest index first.
///
/// The pool rather than the queue: our cores are dedicated and a queue holds 80 copies of one
/// hash, so the pool converges to 8 copies of it and a scan sees an assignment in the very block
/// it lands. It also drains for 8 blocks after a core is freed, which is the handover overlap
/// that keeps packages already in flight valid.
fn scan_pools<'a>(
	pools: impl IntoIterator<Item = &'a AuthPool>,
	wanted: AuthorizerHash,
) -> PoolScan {
	let mut holders = pools
		.into_iter()
		.enumerate()
		.filter(|(_, pool)| pool.contains(&wanted))
		.map(|(core, _)| core as CoreIndex);
	PoolScan { target: holders.next(), also_on: holders.collect() }
}

/// Read the authorizer pools at `anchor` and work out where this para's packages belong.
pub(crate) async fn scan_pools_at<Jam: JamStateSource + ?Sized>(
	jam: &Jam,
	anchor: HeaderHash,
	authorizer: &AuraAuthorizer,
) -> Result<PoolScan, String> {
	let wanted = authorizer.hash();
	let started = Instant::now();
	let pools = jam
		.auth_pools(anchor)
		.await
		.map_err(|error| format!("authorizer pools at {anchor:?}: {error}"))?;
	let elapsed_ms = started.elapsed().as_millis();

	for (core, pool) in pools.iter().enumerate() {
		tracing::debug!(
			target: LOG_TARGET,
			method = "stateValue",
			key = "C(1) AuthPools",
			at = ?anchor,
			core,
			pool_len = pool.len(),
			holds_ours = pool.contains(&wanted),
			copies_of_ours = pool.iter().filter(|hash| **hash == wanted).count(),
			?pool,
			elapsed_ms,
			"JAM read: one core's authorizer pool.",
		);
	}

	let scan = scan_pools(pools.iter(), wanted);
	tracing::debug!(
		target: LOG_TARGET,
		at = ?anchor,
		authorizer_hash = ?wanted,
		cores = pools.len(),
		target = ?scan.target,
		also_on = ?scan.also_on,
		elapsed_ms,
		"Scanned the authorizer pools for this para's authorizer.",
	);
	Ok(scan)
}

/// Run one JAM read, logging the method, the block it was read at, what came back and the
/// round-trip: latency is a first-class debugging signal on this path.
pub(crate) async fn jam_read<T, F>(
	method: &'static str,
	at: HeaderHash,
	read: F,
) -> Result<T, String>
where
	F: Future<Output = jam_interface::Result<T>>,
	T: std::fmt::Debug,
{
	let started = Instant::now();
	let result = read.await;
	let elapsed_ms = started.elapsed().as_millis();
	match &result {
		Ok(value) => tracing::debug!(
			target: LOG_TARGET,
			method,
			?at,
			?value,
			elapsed_ms,
			"JAM read.",
		),
		Err(error) => tracing::warn!(
			target: LOG_TARGET,
			method,
			?at,
			?error,
			elapsed_ms,
			"JAM read failed.",
		),
	}
	result.map_err(|error| format!("{method}: {error}"))
}

/// The head hash a SCALE-encoded `ParaInfo` holds: `head_data` is the header hash of the para's
/// last included block.
pub(crate) fn head_hash_of<Hash: Decode>(para_info: &[u8]) -> Result<Hash, String> {
	let info = ParaInfo::decode(&mut &para_info[..]).map_err(|e| format!("ParaInfo: {e}"))?;
	Hash::decode_all(&mut &info.head_data[..]).map_err(|e| format!("head hash: {e}"))
}

/// Stream of para-head hashes for heads following: every change of the para's `ParaInfo` entry
/// in the parachain service's state yields the new head's header hash together with the slot of
/// the JAM block whose state changed it.
///
/// Absent values (para not registered yet) and undecodable values are dropped, matching the
/// relay-chain streams' behavior of dropping absent heads.
pub(crate) async fn para_head_stream<Hash: Decode + Debug + Send + 'static>(
	jam: &impl JamStateSource,
	parachain_service: ServiceId,
	para_id: u32,
	finalized: bool,
) -> jam_interface::Result<impl Stream<Item = (Hash, JamSlot)> + Send> {
	let key = para_info_key(para_id.into());
	let stream = jam.service_value_stream(parachain_service, &key, finalized).await?;
	Ok(stream.filter_map(move |update| {
		futures::future::ready(match update.value {
			Some(bytes) => match head_hash_of::<Hash>(&bytes) {
				Ok(head) => {
					tracing::debug!(
						target: LOG_TARGET,
						para_id,
						finalized,
						at = ?update.header_hash,
						slot = update.slot,
						?head,
						"Para head changed in JAM state.",
					);
					Some((head, update.slot))
				},
				Err(error) => {
					tracing::warn!(
						target: LOG_TARGET,
						para_id,
						?error,
						"Failed to decode the para head from JAM state; dropping update.",
					);
					None
				},
			},
			None => {
				tracing::debug!(
					target: LOG_TARGET,
					para_id,
					finalized,
					at = ?update.header_hash,
					"Para not (yet) present in the parachain service's state.",
				);
				None
			},
		})
	}))
}

/// Resolve head hashes to headers: a head whose header `lookup` finds is yielded at once, any
/// other waits until its block is imported. Whatever `Extra` travels with a head (the JAM slot
/// it changed at) comes out beside its header.
///
/// A newer head replaces the one waiting, so only the latest head is reported. The stream ends
/// when `heads` ends.
pub(crate) fn resolve_heads<Hash, Header, Extra>(
	heads: impl Stream<Item = (Hash, Extra)> + Send + Unpin + 'static,
	imports: impl Stream<Item = (Hash, Header)> + Send + Unpin + 'static,
	lookup: impl Fn(&Hash) -> Option<Header> + Send + 'static,
) -> impl Stream<Item = (Header, Extra)> + Send
where
	Hash: PartialEq + Send + 'static,
	Header: Send + 'static,
	Extra: Send + 'static,
{
	enum Event<Hash, Header, Extra> {
		Head(Option<(Hash, Extra)>),
		Import(Option<(Hash, Header)>),
	}

	futures::stream::unfold(
		(heads.fuse(), imports.fuse(), lookup, None::<(Hash, Extra)>),
		|(mut heads, mut imports, lookup, mut pending)| async move {
			loop {
				let event = futures::select! {
					head = heads.next() => Event::Head(head),
					import = imports.next() => Event::Import(import),
				};
				match event {
					Event::Head(head) => {
						let (head, extra) = head?;
						match lookup(&head) {
							Some(header) => {
								return Some(((header, extra), (heads, imports, lookup, None)))
							},
							None => pending = Some((head, extra)),
						}
					},
					Event::Import(Some((hash, header)))
						if pending.as_ref().is_some_and(|(waiting, _)| *waiting == hash) =>
					{
						let (_, extra) = pending.take().expect("checked just above; qed");
						return Some(((header, extra), (heads, imports, lookup, None)));
					},
					Event::Import(_) => {},
				}
			}
		},
	)
}

/// The para's head headers, resolved from the head hashes in JAM state through the local
/// database, each with the slot of the JAM block that made it the head; see [`resolve_heads`].
pub(crate) async fn para_header_stream<Block, Client>(
	jam: &(impl JamStateSource + 'static),
	client: Arc<Client>,
	parachain_service: ServiceId,
	para_id: u32,
	finalized: bool,
) -> jam_interface::Result<impl Stream<Item = (Block::Header, JamSlot)> + Send>
where
	Block: BlockT,
	Client: HeaderBackend<Block> + BlockchainEvents<Block> + Send + Sync + 'static,
{
	// Subscribe before opening the JAM stream, so an import racing the first lookup is seen.
	let imports = client.every_import_notification_stream().map(|n| (n.hash, n.header)).boxed();
	let heads = para_head_stream::<Block::Hash>(jam, parachain_service, para_id, finalized).await?;
	Ok(resolve_heads(heads.boxed(), imports, move |hash| client.header(*hash).ok().flatten()))
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use cumulus_client_jam_authorizer::testing::authorizer_of;
	use futures::{channel::mpsc, FutureExt};
	use sp_core::H256;

	fn para_info(head: Vec<u8>) -> Vec<u8> {
		ParaInfo {
			head_data: head.try_into().expect("heads in these tests are below 4 KiB; qed"),
			validation_code: None,
			announced_upgrade: None,
			total_state_balance: 0,
			used_state_balance: 0,
			is_deregistering: false,
		}
		.encode()
	}

	#[test]
	fn the_head_is_the_hash_in_para_info() {
		let hash = H256::repeat_byte(0xab);
		assert_eq!(head_hash_of::<H256>(&para_info(hash.as_bytes().to_vec())), Ok(hash));
	}

	#[test]
	fn an_encoded_header_is_not_a_head_hash() {
		use sp_runtime::traits::Header as _;
		let header = sp_runtime::generic::Header::<u32, sp_runtime::traits::BlakeTwo256>::new(
			1,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		);
		assert!(head_hash_of::<H256>(&para_info(header.encode())).is_err());
	}

	#[test]
	fn bytes_that_are_no_para_info_are_an_error() {
		assert!(head_hash_of::<H256>(&[0xff, 0x00]).is_err());
	}

	type Resolved = std::pin::Pin<Box<dyn Stream<Item = &'static str> + Send>>;

	fn resolver(
		known: Vec<(H256, &'static str)>,
	) -> (mpsc::UnboundedSender<H256>, mpsc::UnboundedSender<(H256, &'static str)>, Resolved) {
		let (heads_tx, heads_rx) = mpsc::unbounded();
		let (imports_tx, imports_rx) = mpsc::unbounded();
		let lookup = move |hash: &H256| {
			known.iter().find(|(known, _)| known == hash).map(|(_, header)| *header)
		};
		let heads = heads_rx.map(|hash| (hash, ()));
		let resolved = resolve_heads(heads, imports_rx, lookup).map(|(header, ())| header);
		(heads_tx, imports_tx, Box::pin(resolved))
	}

	/// What travels with a head comes out beside its header, whether the header was known at
	/// once or had to be imported first.
	#[test]
	fn the_heads_extra_travels_with_its_header() {
		let (a, b) = (H256::repeat_byte(1), H256::repeat_byte(2));
		let (heads, heads_rx) = mpsc::unbounded::<(H256, u32)>();
		let (imports, imports_rx) = mpsc::unbounded::<(H256, &'static str)>();
		let lookup = move |hash: &H256| (*hash == a).then_some("a");
		let mut resolved = Box::pin(resolve_heads(heads_rx, imports_rx, lookup));

		heads
			.unbounded_send((a, 10))
			.expect("the resolver holds the receiving end; qed");
		assert_eq!(resolved.next().now_or_never(), Some(Some(("a", 10))));

		heads
			.unbounded_send((b, 11))
			.expect("the resolver holds the receiving end; qed");
		assert_eq!(resolved.next().now_or_never(), None, "b waits for its import");
		imports
			.unbounded_send((b, "b"))
			.expect("the resolver holds the receiving end; qed");
		assert_eq!(resolved.next().now_or_never(), Some(Some(("b", 11))));
	}

	#[test]
	fn a_known_head_resolves_at_once() {
		let a = H256::repeat_byte(1);
		let (heads, _imports, mut resolved) = resolver(vec![(a, "a")]);
		heads.unbounded_send(a).expect("the resolver holds the receiving end; qed");
		assert_eq!(resolved.next().now_or_never(), Some(Some("a")));
	}

	#[test]
	fn an_unknown_head_resolves_when_imported() {
		let (a, other) = (H256::repeat_byte(1), H256::repeat_byte(9));
		let (heads, imports, mut resolved) = resolver(vec![]);
		heads.unbounded_send(a).expect("the resolver holds the receiving end; qed");
		assert_eq!(resolved.next().now_or_never(), None);
		imports
			.unbounded_send((other, "other"))
			.expect("the resolver holds the receiving end; qed");
		assert_eq!(resolved.next().now_or_never(), None, "an unrelated import yields nothing");
		imports
			.unbounded_send((a, "a"))
			.expect("the resolver holds the receiving end; qed");
		assert_eq!(resolved.next().now_or_never(), Some(Some("a")));
	}

	#[test]
	fn a_newer_head_replaces_the_waiting_one() {
		let (a, b) = (H256::repeat_byte(1), H256::repeat_byte(2));
		let (heads, imports, mut resolved) = resolver(vec![]);
		heads.unbounded_send(a).expect("the resolver holds the receiving end; qed");
		assert_eq!(resolved.next().now_or_never(), None);
		heads.unbounded_send(b).expect("the resolver holds the receiving end; qed");
		assert_eq!(resolved.next().now_or_never(), None);
		imports
			.unbounded_send((a, "a"))
			.expect("the resolver holds the receiving end; qed");
		assert_eq!(resolved.next().now_or_never(), None, "the replaced head is not reported");
		imports
			.unbounded_send((b, "b"))
			.expect("the resolver holds the receiving end; qed");
		assert_eq!(resolved.next().now_or_never(), Some(Some("b")));
	}

	#[test]
	fn the_resolver_ends_with_the_heads() {
		let (heads, _imports, mut resolved) = resolver(vec![]);
		drop(heads);
		assert_eq!(resolved.next().now_or_never(), Some(None));
	}

	/// A finalized JAM chain, newest first, one block per slot ending at `newest_slot`.
	fn finalized_chain(newest_slot: JamSlot, length: u32) -> Vec<BlockDesc> {
		(0..length)
			.map(|back| {
				let slot = newest_slot.saturating_sub(back as JamSlot);
				BlockDesc { header_hash: HeaderHash::from([slot as u8; 32]), slot }
			})
			.collect()
	}

	/// The lookup anchor is the newest finalized block, not a block whose slot names this
	/// collator: under slot-bound sets the *anchor* slot selects the collator, and the set is
	/// announced a session ahead, so there is nothing left to walk for. What remains is
	/// polkajam's freshness rule, applied when the package is reported.
	#[test]
	fn the_lookup_anchor_is_the_newest_finalized_block() {
		let chain = finalized_chain(20, 9);

		let chosen = choose_lookup_anchor(&chain[0], chain[1]).expect("slot 19 is recent enough");

		assert_eq!(chosen.slot, 19, "no walk, the parent of the finalized block itself");
	}

	/// A finalized block that would expire before the package carrying it could be reported is
	/// not a lookup anchor at all: the tick has to be skipped, loudly.
	#[test]
	fn a_stale_lookup_anchor_is_refused() {
		let budget = MAX_LOOKUP_ANCHOR_AGE - LOOKUP_ANCHOR_SAFETY_MARGIN;
		let anchor = BlockDesc { header_hash: HeaderHash::from([1u8; 32]), slot: 100 };

		let newest = |slot| BlockDesc { header_hash: HeaderHash::from([2u8; 32]), slot };
		assert!(choose_lookup_anchor(&anchor, newest(100 - budget)).is_some());
		assert!(choose_lookup_anchor(&anchor, newest(100 - budget - 1)).is_none());
	}

	/// The round-robin is still read off the anchor slot: the set's own size and the para's slot
	/// duration decide whose turn it is, and only that turn may author.
	#[test]
	fn the_round_robin_names_one_collator_per_para_slot() {
		let bob = authorizer_of("alice,bob", "Bob", 3);

		// Para slot = 3 timeslots; bob is collator 1, so timeslots 3..=5 and 9..=11 are his.
		assert!(bob.names_us(3) && bob.names_us(5) && bob.names_us(11));
		assert!(!bob.names_us(2) && !bob.names_us(6));
		assert_eq!(bob.round_robin_window(), 6, "one full turn of two collators");
	}

	/// A runtime that schedules one set change at `switch`, as `pallet-jam-aura` announces it.
	struct ScheduledSet {
		switch: JamSlot,
		current: Vec<AuraPublic<sp_consensus_aura::sr25519::AuthorityId>>,
		next: Vec<AuraPublic<sp_consensus_aura::sr25519::AuthorityId>>,
	}

	impl RuntimeAuthorities<cumulus_test_runtime::Block, sp_consensus_aura::sr25519::AuthorityId>
		for ScheduledSet
	{
		fn authorities_at(
			&self,
			_: H256,
			anchor_slot: JamSlot,
		) -> Result<Vec<AuraPublic<sp_consensus_aura::sr25519::AuthorityId>>, String> {
			Ok(if anchor_slot < self.switch { self.current.clone() } else { self.next.clone() })
		}
	}

	/// The authorizer the collator picks for an anchor slot is the set JAM authorizes there: the
	/// old set before the scheduled switch slot, the new one from the switch slot on. This is what
	/// makes a collator sign with the set the core's pool holds at the package's anchor.
	#[test]
	fn the_authorizer_is_the_sets_at_the_anchor_slot() {
		use cumulus_client_jam_authorizer::testing::{authorities, keystore_of};
		use sp_consensus_aura::sr25519::AuthorityId as Sr25519AuraId;

		let switch = 100;
		let source = Arc::new(ScheduledSet {
			switch,
			current: authorities::<Sr25519AuraId>("alice,bob"),
			next: authorities::<Sr25519AuraId>("charlie,bob"),
		});
		let authorizers = Authorizers::<cumulus_test_runtime::Block, Sr25519AuraId>::new(
			source,
			std::path::PathBuf::from(cumulus_client_jam_authorizer::testing::SOME_BLOB),
			cumulus_client_jam_authorizer::testing::PARA_ID,
			cumulus_client_jam_authorizer::testing::SERVICE_ID,
			1,
			keystore_of::<Sr25519AuraId>("Bob"),
		);

		let before = authorizers
			.for_anchor(H256::zero(), switch - 1)
			.expect("the old set derives")
			.expect("Bob is in the old set");
		let after = authorizers
			.for_anchor(H256::zero(), switch)
			.expect("the new set derives")
			.expect("Bob is in the new set");

		assert_eq!(before.own_index(), 1, "Bob is second in the old set");
		assert_eq!(after.own_index(), 1, "Bob is second in the new set too");
		assert_ne!(
			before.hash(),
			after.hash(),
			"the switch changes the authorizer the core's pool must hold",
		);
	}

	/// A node outside the set at an anchor slot gets `None`, not an error: under slot-bound sets
	/// that is the ordinary state of waiting for a set that names it.
	#[test]
	fn an_anchor_slot_that_does_not_name_us_is_none() {
		use cumulus_client_jam_authorizer::testing::{authorities, keystore_of};
		use sp_consensus_aura::sr25519::AuthorityId as Sr25519AuraId;

		let source = Arc::new(ScheduledSet {
			switch: 10,
			current: authorities::<Sr25519AuraId>("alice,bob"),
			next: authorities::<Sr25519AuraId>("charlie,dave"),
		});
		let authorizers = Authorizers::<cumulus_test_runtime::Block, Sr25519AuraId>::new(
			source,
			std::path::PathBuf::from(cumulus_client_jam_authorizer::testing::SOME_BLOB),
			cumulus_client_jam_authorizer::testing::PARA_ID,
			cumulus_client_jam_authorizer::testing::SERVICE_ID,
			1,
			keystore_of::<Sr25519AuraId>("Bob"),
		);

		assert!(authorizers.for_anchor(H256::zero(), 9).expect("derives").is_some());
		assert!(authorizers.for_anchor(H256::zero(), 10).expect("derives").is_none());
	}

	/// One core's authorizer pool, holding `hashes` as the eight-deep window it is.
	fn pool(hashes: &[AuthorizerHash]) -> AuthPool {
		AuthPool::truncate_from(hashes.to_vec())
	}

	fn other_hash() -> AuthorizerHash {
		AuthorizerHash([0xee; 32])
	}

	/// Nothing waits on JAM to author, but a package can only be submitted to a core whose pool
	/// holds this para's authorizer — anywhere else a guarantor would refuse it. So the scan is
	/// what turns "which core?" from a flag into a fact read off the chain.
	#[test]
	fn a_core_holding_the_paras_authorizer_is_where_its_packages_go() {
		let ours = authorizer_of("alice", "Alice", 1).hash();
		let pools = vec![pool(&[other_hash()]), pool(&[other_hash(), ours])];

		let scan = scan_pools(&pools, ours);

		assert_eq!(scan.target, Some(1));
		assert!(scan.also_on.is_empty());
	}

	/// A core that holds no copy of our hash is not ours, however full its pool is; with none of
	/// them holding it the para has no core at all, which is a state the collator has to survive
	/// rather than crash on — it keeps authoring and submits nothing.
	#[test]
	fn no_core_holding_it_is_no_target_at_all() {
		let ours = authorizer_of("alice", "Alice", 1).hash();

		assert_eq!(scan_pools(&[pool(&[other_hash()]), pool(&[])], ours), PoolScan::default());
		assert_eq!(scan_pools(&[], ours).target, None, "a chain with no cores names none");
	}

	/// Mid-reassignment both the old and the new core hold our hash while the old one's pool
	/// drains. The scan keeps them in index order, which is the order a turn's packages take
	/// them: one package per core, lowest index first.
	#[test]
	fn several_cores_holding_it_go_to_the_lowest_index() {
		let ours = authorizer_of("alice", "Alice", 1).hash();
		let pools = vec![pool(&[ours]), pool(&[other_hash()]), pool(&[ours]), pool(&[ours])];

		let scan = scan_pools(&pools, ours);

		assert_eq!(scan.target, Some(0));
		assert_eq!(scan.also_on, vec![2, 3]);
	}

	/// A turn puts one package on each core that holds the para's authorizer, so the i-th
	/// package needs the i-th core — and the turn has no more packages once they run out.
	#[test]
	fn a_turn_puts_its_packages_on_different_cores() {
		let ours = authorizer_of("alice", "Alice", 1).hash();
		let pools = vec![pool(&[ours]), pool(&[other_hash()]), pool(&[ours]), pool(&[ours])];

		let scan = scan_pools(&pools, ours);

		assert_eq!(scan.core_count(), 3);
		assert_eq!(scan.core_for_package(0), Some(0));
		assert_eq!(scan.core_for_package(1), Some(2));
		assert_eq!(scan.core_for_package(2), Some(3));
		assert_eq!(scan.core_for_package(3), None, "the turn has no core left");
	}

	/// With no core holding it there is no package to place and no core to place it on: the
	/// builder keeps authoring, but the turn produces nothing to submit.
	#[test]
	fn a_para_with_no_core_has_no_packages_to_place() {
		let scan = PoolScan::default();

		assert_eq!(scan.core_count(), 0);
		assert_eq!(scan.core_for_package(0), None);
	}

	/// Two paras on one collator set differ only in their config, so their hashes differ and one
	/// para's core must never look like the other's.
	#[test]
	fn another_paras_core_is_not_ours() {
		let ours = authorizer_of("alice,bob", "Alice", 1).hash();
		let theirs = authorizer_of("alice,bob", "Alice", 2).hash();
		assert_ne!(ours, theirs);

		assert_eq!(scan_pools(&[pool(&[theirs])], ours).target, None);
	}

	/// polkajam refuses a lookup anchor older than `max_lookup_anchor_age`, and it applies that
	/// when the package is *reported*, not when it is built. A package has the report deadline to
	/// get there, so the builder has to keep that much of the window in hand — otherwise the
	/// package dies in flight and looks exactly like one that was never submitted.
	#[test]
	fn a_lookup_anchor_must_outlive_the_packages_time_in_flight() {
		let budget = MAX_LOOKUP_ANCHOR_AGE - LOOKUP_ANCHOR_SAFETY_MARGIN;
		assert!(lookup_anchor_survives_reporting(100, 100 - budget));
		assert!(!lookup_anchor_survives_reporting(100, 100 - budget - 1));
		assert!(lookup_anchor_survives_reporting(0, 0), "a chain younger than the window is fine");
	}

	#[test]
	fn jam_slot_timestamp_is_common_era_based() {
		assert_eq!(jam_slot_timestamp(0).as_millis(), jam_types::JAM_COMMON_ERA * 1000);
		assert_eq!(jam_slot_timestamp(10).as_millis(), (jam_types::JAM_COMMON_ERA + 60) * 1000);
	}

	#[test]
	fn relay_slot_advances_once_per_jam_slot() {
		assert_eq!(jam_slot_as_relay_slot(1), jam_slot_as_relay_slot(0) + 1);
	}

	/// The collation task is the sole writer and the builder reads the latest value at tick time;
	/// clones must share the one counter.
	#[test]
	fn the_authoring_hold_tracks_the_pending_count() {
		let hold = AuthoringHold::new();
		assert_eq!(hold.pending(), 0);

		hold.set_pending(3);
		assert_eq!(hold.pending(), 3);
		assert_eq!(hold.clone().pending(), 3, "clones share the counter");

		hold.set_pending(0);
		assert_eq!(hold.pending(), 0);
	}

	/// The wall-clock-driven builder derives its JAM slot from the clock and its timestamp from
	/// that slot, so the two conversions have to be exact inverses.
	#[test]
	fn jam_slot_at_inverts_jam_slot_timestamp() {
		for slot in [0, 1, 12_345] {
			assert_eq!(jam_slot_at(jam_slot_timestamp(slot)), slot);
		}
	}

	/// Anything inside a slot belongs to that slot; only the next boundary moves it on.
	#[test]
	fn jam_slot_at_floors_to_the_slot_start() {
		let slot_start = jam_slot_timestamp(7).as_millis();
		assert_eq!(jam_slot_at(Timestamp::new(slot_start + JAM_SLOT_DURATION_MS - 1)), 7);
		assert_eq!(jam_slot_at(Timestamp::new(slot_start + JAM_SLOT_DURATION_MS)), 8);
	}

	/// Before the common era there are no slots; clamping to slot 0 keeps a badly-set clock from
	/// wrapping the subtraction.
	#[test]
	fn jam_slot_at_clamps_before_the_common_era() {
		assert_eq!(jam_slot_at(Timestamp::new(0)), 0);
	}

	/// A node with no JAM state behind it proves every key absent, so the runtime falls back
	/// instead of trapping on a read the node cannot answer.
	#[test]
	fn no_jam_state_reader_proves_every_key_absent() {
		assert_eq!(NoJamStateReader.read(&[0xAB; 31]), None);
	}
}
