// Copyright 2018-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Aura (Authority-round) consensus in substrate.
//!
//! Aura works by having a list of authorities A who are expected to roughly
//! agree on the current time. Time is divided up into discrete slots of t
//! seconds each. For each slot s, the author of that slot is A[s % |A|].
//!
//! The author is allowed to issue one block but not more during that slot,
//! and it will be built upon the longest valid chain that has been seen.
//!
//! Blocks from future steps will be either deferred or rejected depending on how
//! far in the future they are.
//!
//! NOTE: Aura itself is designed to be generic over the crypto used.
#![forbid(missing_docs, unsafe_code)]
use std::{
	sync::Arc, time::Duration, thread, marker::PhantomData, hash::Hash, fmt::Debug, pin::Pin,
	collections::HashMap
};

use futures::prelude::*;
use parking_lot::Mutex;
use log::{debug, info, trace};

use codec::{Encode, Decode, Codec};

use sp_consensus::{
	self, BlockImport, Environment, Proposer, CanAuthorWith, ForkChoiceStrategy, BlockImportParams,
	BlockOrigin, Error as ConsensusError, SelectChain, SlotData, BlockCheckParams, ImportResult
};
use sp_consensus::import_queue::{
	Verifier, BasicQueue, BoxJustificationImport, BoxFinalityProofImport,
};
use sc_client_api::backend::AuxStore;
use sc_client::BlockOf;
use sp_blockchain::{
	self, Result as CResult, well_known_cache_keys::{self, Id as CacheKeyId},
	ProvideCache, HeaderBackend,
};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_runtime::{generic::{BlockId, OpaqueDigestItemId}, Justification};
use sp_runtime::traits::{Block as BlockT, Header, DigestItemFor, ProvideRuntimeApi, Zero, Member};
use sp_core::crypto::Pair;
use sp_inherents::{InherentDataProviders, InherentData};
use sp_timestamp::{
	TimestampInherentData, InherentType as TimestampInherent, InherentError as TIError
};
use sc_telemetry::{telemetry, CONSENSUS_TRACE, CONSENSUS_DEBUG, CONSENSUS_INFO};
use sc_consensus_slots::{CheckedHeader, SlotWorker, SlotInfo, SlotCompatible};
use sc_consensus_slots::check_equivocation;
use sc_keystore::KeyStorePtr;
use sp_api::ApiExt;

pub use sp_consensus_aura::{
	ConsensusLog, AuraApi, AURA_ENGINE_ID,
	inherents::{
		InherentType as AuraInherent,
		AuraInherentData, INHERENT_IDENTIFIER, InherentDataProvider,
	},
};
pub use sp_consensus::SyncOracle;
pub use digest::CompatibleDigestItem;

mod digest;

type AuthorityId<P> = <P as Pair>::Public;

/// A slot duration. Create with `get_or_compute`.
#[derive(Clone, Copy, Debug, Encode, Decode, Hash, PartialOrd, Ord, PartialEq, Eq)]
pub struct SlotDuration(sc_consensus_slots::SlotDuration<u64>);

impl SlotDuration {
	/// Either fetch the slot duration from disk or compute it from the genesis
	/// state.
	pub fn get_or_compute<A, B, C>(client: &C) -> CResult<Self>
	where
		A: Codec,
		B: BlockT,
		C: AuxStore + ProvideRuntimeApi,
		C::Api: AuraApi<B, A, Error = sp_blockchain::Error>,
	{
		sc_consensus_slots::SlotDuration::get_or_compute(client, |a, b| a.slot_duration(b)).map(Self)
	}

	/// Get the slot duration in milliseconds.
	pub fn get(&self) -> u64 {
		self.0.get()
	}
}

/// Get slot author for given block along with authorities.
fn slot_author<P: Pair>(slot_num: u64, authorities: &[AuthorityId<P>]) -> Option<&AuthorityId<P>> {
	if authorities.is_empty() { return None }

	let idx = slot_num % (authorities.len() as u64);
	assert!(
		idx <= usize::max_value() as u64,
		"It is impossible to have a vector with length beyond the address space; qed",
	);

	let current_author = authorities.get(idx as usize)
		.expect("authorities not empty; index constrained to list length;\
				this is a valid index; qed");

	Some(current_author)
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
struct AuraSlotCompatible;

impl SlotCompatible for AuraSlotCompatible {
	fn extract_timestamp_and_slot(
		&self,
		data: &InherentData
	) -> Result<(TimestampInherent, AuraInherent, std::time::Duration), sp_consensus::Error> {
		data.timestamp_inherent_data()
			.and_then(|t| data.aura_inherent_data().map(|a| (t, a)))
			.map_err(Into::into)
			.map_err(sp_consensus::Error::InherentData)
			.map(|(x, y)| (x, y, Default::default()))
	}
}

/// Start the aura worker. The returned future should be run in a futures executor.
pub fn start_aura<B, C, SC, E, I, P, SO, CAW, Error, H>(
	slot_duration: SlotDuration,
	client: Arc<C>,
	select_chain: SC,
	block_import: I,
	env: E,
	sync_oracle: SO,
	inherent_data_providers: InherentDataProviders,
	force_authoring: bool,
	keystore: KeyStorePtr,
	can_author_with: CAW,
) -> Result<impl futures01::Future<Item = (), Error = ()>, sp_consensus::Error> where
	B: BlockT<Header=H>,
	C: ProvideRuntimeApi + BlockOf + ProvideCache<B> + AuxStore + Send + Sync,
	C::Api: AuraApi<B, AuthorityId<P>>,
	SC: SelectChain<B>,
	E: Environment<B, Error=Error> + Send + Sync + 'static,
	E::Proposer: Proposer<B, Error=Error>,
	<E::Proposer as Proposer<B>>::Create: Unpin + Send,
	P: Pair + Send + Sync,
	P::Public: Hash + Member + Encode + Decode,
	P::Signature: Hash + Member + Encode + Decode,
	H: Header<Hash=B::Hash>,
	I: BlockImport<B> + Send + Sync + 'static,
	Error: ::std::error::Error + Send + From<::sp_consensus::Error> + From<I::Error> + 'static,
	SO: SyncOracle + Send + Sync + Clone,
	CAW: CanAuthorWith<B> + Send,
{
	let worker = AuraWorker {
		client: client.clone(),
		block_import: Arc::new(Mutex::new(block_import)),
		env,
		keystore,
		sync_oracle: sync_oracle.clone(),
		force_authoring,
		_key_type: PhantomData::<P>,
	};
	register_aura_inherent_data_provider(
		&inherent_data_providers,
		slot_duration.0.slot_duration()
	)?;
	Ok(sc_consensus_slots::start_slot_worker::<_, _, _, _, _, AuraSlotCompatible, _>(
		slot_duration.0,
		select_chain,
		worker,
		sync_oracle,
		inherent_data_providers,
		AuraSlotCompatible,
		can_author_with,
	).map(|()| Ok::<(), ()>(())).compat())
}

struct AuraWorker<C, E, I, P, SO> {
	client: Arc<C>,
	block_import: Arc<Mutex<I>>,
	env: E,
	keystore: KeyStorePtr,
	sync_oracle: SO,
	force_authoring: bool,
	_key_type: PhantomData<P>,
}

impl<H, B, C, E, I, P, Error, SO> sc_consensus_slots::SimpleSlotWorker<B> for AuraWorker<C, E, I, P, SO> where
	B: BlockT<Header=H>,
	C: ProvideRuntimeApi + BlockOf + ProvideCache<B> + Sync,
	C::Api: AuraApi<B, AuthorityId<P>>,
	E: Environment<B, Error=Error>,
	E::Proposer: Proposer<B, Error=Error>,
	<E::Proposer as Proposer<B>>::Create: Unpin + Send,
	H: Header<Hash=B::Hash>,
	I: BlockImport<B> + Send + Sync + 'static,
	P: Pair + Send + Sync,
	P::Public: Member + Encode + Decode + Hash,
	P::Signature: Member + Encode + Decode + Hash + Debug,
	SO: SyncOracle + Send + Clone,
	Error: ::std::error::Error + Send + From<::sp_consensus::Error> + From<I::Error> + 'static,
{
	type BlockImport = I;
	type SyncOracle = SO;
	type Proposer = E::Proposer;
	type Claim = P;
	type EpochData = Vec<AuthorityId<P>>;

	fn logging_target(&self) -> &'static str {
		"aura"
	}

	fn block_import(&self) -> Arc<Mutex<Self::BlockImport>> {
		self.block_import.clone()
	}

	fn epoch_data(&self, header: &B::Header, _slot_number: u64) -> Result<Self::EpochData, sp_consensus::Error> {
		authorities(self.client.as_ref(), &BlockId::Hash(header.hash()))
	}

	fn authorities_len(&self, epoch_data: &Self::EpochData) -> usize {
		epoch_data.len()
	}

	fn claim_slot(
		&self,
		_header: &B::Header,
		slot_number: u64,
		epoch_data: &Self::EpochData,
	) -> Option<Self::Claim> {
		let expected_author = slot_author::<P>(slot_number, epoch_data);

		expected_author.and_then(|p| {
			self.keystore.read()
				.key_pair_by_type::<P>(&p, sp_application_crypto::key_types::AURA).ok()
		})
	}

	fn pre_digest_data(&self, slot_number: u64, _claim: &Self::Claim) -> Vec<sp_runtime::DigestItem<B::Hash>> {
		vec![
			<DigestItemFor<B> as CompatibleDigestItem<P>>::aura_pre_digest(slot_number),
		]
	}

	fn block_import_params(&self) -> Box<dyn Fn(
		B::Header,
		&B::Hash,
		Vec<B::Extrinsic>,
		Self::Claim,
	) -> sp_consensus::BlockImportParams<B> + Send> {
		Box::new(|header, header_hash, body, pair| {
			// sign the pre-sealed hash of the block and then
			// add it to a digest item.
			let signature = pair.sign(header_hash.as_ref());
			let signature_digest_item = <DigestItemFor<B> as CompatibleDigestItem<P>>::aura_seal(signature);

			BlockImportParams {
				origin: BlockOrigin::Own,
				header,
				justification: None,
				post_digests: vec![signature_digest_item],
				body: Some(body),
				finalized: false,
				auxiliary: Vec::new(),
				fork_choice: ForkChoiceStrategy::LongestChain,
				allow_missing_state: false,
				import_existing: false,
			}
		})
	}

	fn force_authoring(&self) -> bool {
		self.force_authoring
	}

	fn sync_oracle(&mut self) -> &mut Self::SyncOracle {
		&mut self.sync_oracle
	}

	fn proposer(&mut self, block: &B::Header) -> Result<Self::Proposer, sp_consensus::Error> {
		self.env.init(block).map_err(|e| {
			sp_consensus::Error::ClientImport(format!("{:?}", e)).into()
		})
	}

	fn proposing_remaining_duration(
		&self,
		head: &B::Header,
		slot_info: &SlotInfo
	) -> Option<std::time::Duration> {
		// never give more than 20 times more lenience.
		const BACKOFF_CAP: u64 = 20;

		let slot_remaining = self.slot_remaining_duration(slot_info);
		let parent_slot = match find_pre_digest::<B, P>(head) {
			Err(_) => return Some(slot_remaining),
			Ok(d) => d,
		};

		// we allow a lenience of the number of slots since the head of the
		// chain was produced, minus 1 (since there is always a difference of at least 1)
		//
		// linear back-off.
		// in normal cases we only attempt to issue blocks up to the end of the slot.
		// when the chain has been stalled for a few slots, we give more lenience.
		let slot_lenience = slot_info.number.saturating_sub(parent_slot + 1);
		let slot_lenience = std::cmp::min(slot_lenience, BACKOFF_CAP);
		let slot_lenience = Duration::from_secs(slot_lenience * slot_info.duration);
		Some(slot_lenience + slot_remaining)
	}
}

impl<H, B: BlockT, C, E, I, P, Error, SO> SlotWorker<B> for AuraWorker<C, E, I, P, SO> where
	B: BlockT<Header=H>,
	C: ProvideRuntimeApi + BlockOf + ProvideCache<B> + Sync + Send,
	C::Api: AuraApi<B, AuthorityId<P>>,
	E: Environment<B, Error=Error> + Send + Sync,
	E::Proposer: Proposer<B, Error=Error>,
	<E::Proposer as Proposer<B>>::Create: Unpin + Send + 'static,
	H: Header<Hash=B::Hash>,
	I: BlockImport<B> + Send + Sync + 'static,
	P: Pair + Send + Sync,
	P::Public: Member + Encode + Decode + Hash,
	P::Signature: Member + Encode + Decode + Hash + Debug,
	SO: SyncOracle + Send + Sync + Clone,
	Error: ::std::error::Error + Send + From<::sp_consensus::Error> + From<I::Error> + 'static,
{
	type OnSlot = Pin<Box<dyn Future<Output = Result<(), sp_consensus::Error>> + Send>>;

	fn on_slot(&mut self, chain_head: B::Header, slot_info: SlotInfo) -> Self::OnSlot {
		<Self as sc_consensus_slots::SimpleSlotWorker<B>>::on_slot(self, chain_head, slot_info)
	}
}

fn aura_err<B: BlockT>(error: Error<B>) -> Error<B> {
	debug!(target: "aura", "{}", error);
	error
}

#[derive(derive_more::Display, Debug)]
enum Error<B: BlockT> {
	#[display(fmt = "Multiple Aura pre-runtime headers")]
	MultipleHeaders,
	#[display(fmt = "No Aura pre-runtime digest found")]
	NoDigestFound,
	#[display(fmt = "Header {:?} is unsealed", _0)]
	HeaderUnsealed(B::Hash),
	#[display(fmt = "Header {:?} has a bad seal", _0)]
	HeaderBadSeal(B::Hash),
	#[display(fmt = "Slot Author not found")]
	SlotAuthorNotFound,
	#[display(fmt = "Bad signature on {:?}", _0)]
	BadSignature(B::Hash),
	#[display(fmt = "Rejecting block too far in future")]
	TooFarInFuture,
	Client(sp_blockchain::Error),
	DataProvider(String),
	Runtime(String),
	#[display(fmt = "Slot number must increase: parent slot: {}, this slot: {}", _0, _1)]
	SlotNumberMustIncrease(u64, u64),
	#[display(fmt = "Parent ({}) of {} unavailable. Cannot import", _0, _1)]
	ParentUnavailable(B::Hash, B::Hash),
}

impl<B: BlockT> std::convert::From<Error<B>> for String {
	fn from(error: Error<B>) -> String {
		error.to_string()
	}
}

fn find_pre_digest<B: BlockT, P: Pair>(header: &B::Header) -> Result<u64, Error<B>>
	where DigestItemFor<B>: CompatibleDigestItem<P>,
		P::Signature: Decode,
		P::Public: Encode + Decode + PartialEq + Clone,
{
	if header.number().is_zero() {
		return Ok(0);
	}

	let mut pre_digest: Option<u64> = None;
	for log in header.digest().logs() {
		trace!(target: "aura", "Checking log {:?}", log);
		match (log.as_aura_pre_digest(), pre_digest.is_some()) {
			(Some(_), true) => Err(aura_err(Error::MultipleHeaders))?,
			(None, _) => trace!(target: "aura", "Ignoring digest not meant for us"),
			(s, false) => pre_digest = s,
		}
	}
	pre_digest.ok_or_else(|| aura_err(Error::NoDigestFound))
}

/// check a header has been signed by the right key. If the slot is too far in the future, an error will be returned.
/// if it's successful, returns the pre-header and the digest item containing the seal.
///
/// This digest item will always return `Some` when used with `as_aura_seal`.
//
// FIXME #1018 needs misbehavior types. The `transaction_pool` parameter will be
// used to submit such misbehavior reports.
fn check_header<C, B: BlockT, P: Pair, T>(
	client: &C,
	slot_now: u64,
	mut header: B::Header,
	hash: B::Hash,
	authorities: &[AuthorityId<P>],
	_transaction_pool: Option<&T>,
) -> Result<CheckedHeader<B::Header, (u64, DigestItemFor<B>)>, Error<B>> where
	DigestItemFor<B>: CompatibleDigestItem<P>,
	P::Signature: Decode,
	C: sc_client_api::backend::AuxStore,
	P::Public: Encode + Decode + PartialEq + Clone,
	T: Send + Sync + 'static,
{
	let seal = match header.digest_mut().pop() {
		Some(x) => x,
		None => return Err(Error::HeaderUnsealed(hash)),
	};

	let sig = seal.as_aura_seal().ok_or_else(|| {
		aura_err(Error::HeaderBadSeal(hash))
	})?;

	let slot_num = find_pre_digest::<B, _>(&header)?;

	if slot_num > slot_now {
		header.digest_mut().push(seal);
		Ok(CheckedHeader::Deferred(header, slot_num))
	} else {
		// check the signature is valid under the expected authority and
		// chain state.
		let expected_author = match slot_author::<P>(slot_num, &authorities) {
			None => return Err(Error::SlotAuthorNotFound),
			Some(author) => author,
		};

		let pre_hash = header.hash();

		if P::verify(&sig, pre_hash.as_ref(), expected_author) {
			if let Some(equivocation_proof) = check_equivocation(
				client,
				slot_now,
				slot_num,
				&header,
				expected_author,
			).map_err(Error::Client)? {
				info!(
					"Slot author is equivocating at slot {} with headers {:?} and {:?}",
					slot_num,
					equivocation_proof.fst_header().hash(),
					equivocation_proof.snd_header().hash(),
				);
			}

			Ok(CheckedHeader::Checked(header, (slot_num, seal)))
		} else {
			Err(Error::BadSignature(hash))
		}
	}
}

/// A verifier for Aura blocks.
pub struct AuraVerifier<C, P, T> {
	client: Arc<C>,
	phantom: PhantomData<P>,
	inherent_data_providers: sp_inherents::InherentDataProviders,
	transaction_pool: Option<Arc<T>>,
}

impl<C, P, T> AuraVerifier<C, P, T>
	where P: Send + Sync + 'static
{
	fn check_inherents<B: BlockT>(
		&self,
		block: B,
		block_id: BlockId<B>,
		inherent_data: InherentData,
		timestamp_now: u64,
	) -> Result<(), Error<B>>
		where C: ProvideRuntimeApi, C::Api: BlockBuilderApi<B, Error = sp_blockchain::Error>
	{
		const MAX_TIMESTAMP_DRIFT_SECS: u64 = 60;

		let inherent_res = self.client.runtime_api().check_inherents(
			&block_id,
			block,
			inherent_data,
		).map_err(Error::Client)?;

		if !inherent_res.ok() {
			inherent_res
				.into_errors()
				.try_for_each(|(i, e)| match TIError::try_from(&i, &e) {
					Some(TIError::ValidAtTimestamp(timestamp)) => {
						// halt import until timestamp is valid.
						// reject when too far ahead.
						if timestamp > timestamp_now + MAX_TIMESTAMP_DRIFT_SECS {
							return Err(Error::TooFarInFuture);
						}

						let diff = timestamp.saturating_sub(timestamp_now);
						info!(
							target: "aura",
							"halting for block {} seconds in the future",
							diff
						);
						telemetry!(CONSENSUS_INFO; "aura.halting_for_future_block";
							"diff" => ?diff
						);
						thread::sleep(Duration::from_secs(diff));
						Ok(())
					},
					Some(TIError::Other(e)) => Err(Error::Runtime(e.into())),
					None => Err(Error::DataProvider(
						self.inherent_data_providers.error_to_string(&i, &e)
					)),
				})
		} else {
			Ok(())
		}
	}
}

#[forbid(deprecated)]
impl<B: BlockT, C, P, T> Verifier<B> for AuraVerifier<C, P, T> where
	C: ProvideRuntimeApi + Send + Sync + sc_client_api::backend::AuxStore + ProvideCache<B> + BlockOf,
	C::Api: BlockBuilderApi<B> + AuraApi<B, AuthorityId<P>> + ApiExt<B, Error = sp_blockchain::Error>,
	DigestItemFor<B>: CompatibleDigestItem<P>,
	P: Pair + Send + Sync + 'static,
	P::Public: Send + Sync + Hash + Eq + Clone + Decode + Encode + Debug + 'static,
	P::Signature: Encode + Decode,
	T: Send + Sync + 'static,
{
	fn verify(
		&mut self,
		origin: BlockOrigin,
		header: B::Header,
		justification: Option<Justification>,
		mut body: Option<Vec<B::Extrinsic>>,
	) -> Result<(BlockImportParams<B>, Option<Vec<(CacheKeyId, Vec<u8>)>>), String> {
		let mut inherent_data = self.inherent_data_providers
			.create_inherent_data()
			.map_err(|e| e.into_string())?;
		let (timestamp_now, slot_now, _) = AuraSlotCompatible.extract_timestamp_and_slot(&inherent_data)
			.map_err(|e| format!("Could not extract timestamp and slot: {:?}", e))?;
		let hash = header.hash();
		let parent_hash = *header.parent_hash();
		let authorities = authorities(self.client.as_ref(), &BlockId::Hash(parent_hash))
			.map_err(|e| format!("Could not fetch authorities at {:?}: {:?}", parent_hash, e))?;

		// we add one to allow for some small drift.
		// FIXME #1019 in the future, alter this queue to allow deferring of
		// headers
		let checked_header = check_header::<C, B, P, T>(
			&self.client,
			slot_now + 1,
			header,
			hash,
			&authorities[..],
			self.transaction_pool.as_ref().map(|x| &**x),
		).map_err(|e| e.to_string())?;
		match checked_header {
			CheckedHeader::Checked(pre_header, (slot_num, seal)) => {
				// if the body is passed through, we need to use the runtime
				// to check that the internally-set timestamp in the inherents
				// actually matches the slot set in the seal.
				if let Some(inner_body) = body.take() {
					inherent_data.aura_replace_inherent_data(slot_num);
					let block = B::new(pre_header.clone(), inner_body);

					// skip the inherents verification if the runtime API is old.
					if self.client
						.runtime_api()
						.has_api_with::<dyn BlockBuilderApi<B, Error = ()>, _>(
							&BlockId::Hash(parent_hash),
							|v| v >= 2,
						)
						.map_err(|e| format!("{:?}", e))?
					{
						self.check_inherents(
							block.clone(),
							BlockId::Hash(parent_hash),
							inherent_data,
							timestamp_now,
						).map_err(|e| e.to_string())?;
					}

					let (_, inner_body) = block.deconstruct();
					body = Some(inner_body);
				}

				trace!(target: "aura", "Checked {:?}; importing.", pre_header);
				telemetry!(CONSENSUS_TRACE; "aura.checked_and_importing"; "pre_header" => ?pre_header);

				// Look for an authorities-change log.
				let maybe_keys = pre_header.digest()
					.logs()
					.iter()
					.filter_map(|l| l.try_to::<ConsensusLog<AuthorityId<P>>>(
						OpaqueDigestItemId::Consensus(&AURA_ENGINE_ID)
					))
					.find_map(|l| match l {
						ConsensusLog::AuthoritiesChange(a) => Some(
							vec![(well_known_cache_keys::AUTHORITIES, a.encode())]
						),
						_ => None,
					});

				let block_import_params = BlockImportParams {
					origin,
					header: pre_header,
					post_digests: vec![seal],
					body,
					finalized: false,
					justification,
					auxiliary: Vec::new(),
					fork_choice: ForkChoiceStrategy::LongestChain,
					allow_missing_state: false,
					import_existing: false,
				};

				Ok((block_import_params, maybe_keys))
			}
			CheckedHeader::Deferred(a, b) => {
				debug!(target: "aura", "Checking {:?} failed; {:?}, {:?}.", hash, a, b);
				telemetry!(CONSENSUS_DEBUG; "aura.header_too_far_in_future";
					"hash" => ?hash, "a" => ?a, "b" => ?b
				);
				Err(format!("Header {:?} rejected: too far in the future", hash))
			}
		}
	}
}

fn initialize_authorities_cache<A, B, C>(client: &C) -> Result<(), ConsensusError> where
	A: Codec,
	B: BlockT,
	C: ProvideRuntimeApi + BlockOf + ProvideCache<B>,
	C::Api: AuraApi<B, A>,
{
	// no cache => no initialization
	let cache = match client.cache() {
		Some(cache) => cache,
		None => return Ok(()),
	};

	// check if we already have initialized the cache
	let genesis_id = BlockId::Number(Zero::zero());
	let genesis_authorities: Option<Vec<A>> = cache
		.get_at(&well_known_cache_keys::AUTHORITIES, &genesis_id)
		.and_then(|(_, _, v)| Decode::decode(&mut &v[..]).ok());
	if genesis_authorities.is_some() {
		return Ok(());
	}

	let map_err = |error| sp_consensus::Error::from(sp_consensus::Error::ClientImport(
		format!(
			"Error initializing authorities cache: {}",
			error,
		)));
	let genesis_authorities = authorities(client, &genesis_id)?;
	cache.initialize(&well_known_cache_keys::AUTHORITIES, genesis_authorities.encode())
		.map_err(map_err)?;

	Ok(())
}

#[allow(deprecated)]
fn authorities<A, B, C>(client: &C, at: &BlockId<B>) -> Result<Vec<A>, ConsensusError> where
	A: Codec,
	B: BlockT,
	C: ProvideRuntimeApi + BlockOf + ProvideCache<B>,
	C::Api: AuraApi<B, A>,
{
	client
		.cache()
		.and_then(|cache| cache
			.get_at(&well_known_cache_keys::AUTHORITIES, at)
			.and_then(|(_, _, v)| Decode::decode(&mut &v[..]).ok())
		)
		.or_else(|| AuraApi::authorities(&*client.runtime_api(), at).ok())
		.ok_or_else(|| sp_consensus::Error::InvalidAuthoritiesSet.into())
}

/// The Aura import queue type.
pub type AuraImportQueue<B> = BasicQueue<B>;

/// Register the aura inherent data provider, if not registered already.
fn register_aura_inherent_data_provider(
	inherent_data_providers: &InherentDataProviders,
	slot_duration: u64,
) -> Result<(), sp_consensus::Error> {
	if !inherent_data_providers.has_provider(&INHERENT_IDENTIFIER) {
		inherent_data_providers
			.register_provider(InherentDataProvider::new(slot_duration))
			.map_err(Into::into)
			.map_err(sp_consensus::Error::InherentData)
	} else {
		Ok(())
	}
}

/// A block-import handler for Aura.
pub struct AuraBlockImport<Block: BlockT, C, I: BlockImport<Block>, P> {
	inner: I,
	client: Arc<C>,
	_phantom: PhantomData<(Block, P)>,
}

impl<Block: BlockT, C, I: Clone + BlockImport<Block>, P> Clone for AuraBlockImport<Block, C, I, P> {
	fn clone(&self) -> Self {
		AuraBlockImport {
			inner: self.inner.clone(),
			client: self.client.clone(),
			_phantom: PhantomData,
		}
	}
}

impl<Block: BlockT, C, I: BlockImport<Block>, P> AuraBlockImport<Block, C, I, P> {
	/// New aura block import.
	pub fn new(
		inner: I,
		client: Arc<C>,
	) -> Self {
		Self {
			inner,
			client,
			_phantom: PhantomData,
		}
	}
}

impl<Block: BlockT, C, I, P> BlockImport<Block> for AuraBlockImport<Block, C, I, P> where
	I: BlockImport<Block> + Send + Sync,
	I::Error: Into<ConsensusError>,
	C: HeaderBackend<Block>,
	P: Pair + Send + Sync + 'static,
	P::Public: Clone + Eq + Send + Sync + Hash + Debug + Encode + Decode,
	P::Signature: Encode + Decode,
{
	type Error = ConsensusError;

	fn check_block(
		&mut self,
		block: BlockCheckParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).map_err(Into::into)
	}

	fn import_block(
		&mut self,
		block: BlockImportParams<Block>,
		new_cache: HashMap<CacheKeyId, Vec<u8>>,
	) -> Result<ImportResult, Self::Error> {
		let hash = block.post_header().hash();
		let slot_number = find_pre_digest::<Block, P>(&block.header)
			.expect("valid Aura headers must contain a predigest; \
					 header has been already verified; qed");

		let parent_hash = *block.header.parent_hash();
		let parent_header = self.client.header(BlockId::Hash(parent_hash))
			.map_err(|e| ConsensusError::ChainLookup(e.to_string()))?
			.ok_or_else(|| ConsensusError::ChainLookup(aura_err(
				Error::<Block>::ParentUnavailable(parent_hash, hash)
			).into()))?;

		let parent_slot = find_pre_digest::<Block, P>(&parent_header)
			.expect("valid Aura headers contain a pre-digest; \
					parent header has already been verified; qed");

		// make sure that slot number is strictly increasing
		if slot_number <= parent_slot {
			return Err(
				ConsensusError::ClientImport(aura_err(
					Error::<Block>::SlotNumberMustIncrease(parent_slot, slot_number)
				).into())
			);
		}

		self.inner.import_block(block, new_cache)
			.map_err(Into::into)
	}
}

/// Start an import queue for the Aura consensus algorithm.
pub fn import_queue<B, I, C, P, T>(
	slot_duration: SlotDuration,
	block_import: I,
	justification_import: Option<BoxJustificationImport<B>>,
	finality_proof_import: Option<BoxFinalityProofImport<B>>,
	client: Arc<C>,
	inherent_data_providers: InherentDataProviders,
	transaction_pool: Option<Arc<T>>,
) -> Result<AuraImportQueue<B>, sp_consensus::Error> where
	B: BlockT,
	C::Api: BlockBuilderApi<B> + AuraApi<B, AuthorityId<P>> + ApiExt<B, Error = sp_blockchain::Error>,
	C: 'static + ProvideRuntimeApi + BlockOf + ProvideCache<B> + Send + Sync + AuxStore + HeaderBackend<B>,
	I: BlockImport<B,Error=ConsensusError> + Send + Sync + 'static,
	DigestItemFor<B>: CompatibleDigestItem<P>,
	P: Pair + Send + Sync + 'static,
	P::Public: Clone + Eq + Send + Sync + Hash + Debug + Encode + Decode,
	P::Signature: Encode + Decode,
	T: Send + Sync + 'static,
{
	register_aura_inherent_data_provider(&inherent_data_providers, slot_duration.get())?;
	initialize_authorities_cache(&*client)?;

	let verifier = AuraVerifier {
		client: client.clone(),
		inherent_data_providers,
		phantom: PhantomData,
		transaction_pool,
	};
	Ok(BasicQueue::new(
		verifier,
		Box::new(block_import),
		justification_import,
		finality_proof_import,
	))
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_consensus::NoNetwork as DummyOracle;
	use sc_network_test::{Block as TestBlock, *};
	use sp_runtime::traits::{Block as BlockT, DigestFor};
	use sc_network::config::ProtocolConfig;
	use parking_lot::Mutex;
	use tokio::runtime::current_thread;
	use sp_keyring::sr25519::Keyring;
	use sc_client::BlockchainEvents;
	use sp_consensus_aura::sr25519::AuthorityPair;

	type Error = sp_blockchain::Error;

	type TestClient = sc_client::Client<
		substrate_test_runtime_client::Backend,
		substrate_test_runtime_client::Executor,
		TestBlock,
		substrate_test_runtime_client::runtime::RuntimeApi
	>;

	struct DummyFactory(Arc<TestClient>);
	struct DummyProposer(u64, Arc<TestClient>);

	impl Environment<TestBlock> for DummyFactory {
		type Proposer = DummyProposer;
		type Error = Error;

		fn init(&mut self, parent_header: &<TestBlock as BlockT>::Header)
			-> Result<DummyProposer, Error>
		{
			Ok(DummyProposer(parent_header.number + 1, self.0.clone()))
		}
	}

	impl Proposer<TestBlock> for DummyProposer {
		type Error = Error;
		type Create = future::Ready<Result<TestBlock, Error>>;

		fn propose(
			&mut self,
			_: InherentData,
			digests: DigestFor<TestBlock>,
			_: Duration,
		) -> Self::Create {
			let r = self.1.new_block(digests).unwrap().bake().map_err(|e| e.into());
			future::ready(r)
		}
	}

	const SLOT_DURATION: u64 = 1000;

	pub struct AuraTestNet {
		peers: Vec<Peer<(), DummySpecialization>>,
	}

	impl TestNetFactory for AuraTestNet {
		type Specialization = DummySpecialization;
		type Verifier = AuraVerifier<PeersFullClient, AuthorityPair, ()>;
		type PeerData = ();

		/// Create new test network with peers and given config.
		fn from_config(_config: &ProtocolConfig) -> Self {
			AuraTestNet {
				peers: Vec::new(),
			}
		}

		fn make_verifier(&self, client: PeersClient, _cfg: &ProtocolConfig, _peer_data: &())
			-> Self::Verifier
		{
			match client {
				PeersClient::Full(client, _) => {
					let slot_duration = SlotDuration::get_or_compute(&*client)
						.expect("slot duration available");
					let inherent_data_providers = InherentDataProviders::new();
					register_aura_inherent_data_provider(
						&inherent_data_providers,
						slot_duration.get()
					).expect("Registers aura inherent data provider");

					assert_eq!(slot_duration.get(), SLOT_DURATION);
					AuraVerifier {
						client,
						inherent_data_providers,
						transaction_pool: Default::default(),
						phantom: Default::default(),
					}
				},
				PeersClient::Light(_, _) => unreachable!("No (yet) tests for light client + Aura"),
			}
		}

		fn peer(&mut self, i: usize) -> &mut Peer<Self::PeerData, DummySpecialization> {
			&mut self.peers[i]
		}

		fn peers(&self) -> &Vec<Peer<Self::PeerData, DummySpecialization>> {
			&self.peers
		}

		fn mut_peers<F: FnOnce(&mut Vec<Peer<Self::PeerData, DummySpecialization>>)>(&mut self, closure: F) {
			closure(&mut self.peers);
		}
	}

	#[test]
	#[allow(deprecated)]
	fn authoring_blocks() {
		let _ = env_logger::try_init();
		let net = AuraTestNet::new(3);

		let peers = &[
			(0, Keyring::Alice),
			(1, Keyring::Bob),
			(2, Keyring::Charlie),
		];

		let net = Arc::new(Mutex::new(net));
		let mut import_notifications = Vec::new();

		let mut runtime = current_thread::Runtime::new().unwrap();
		let mut keystore_paths = Vec::new();
		for (peer_id, key) in peers {
			let mut net = net.lock();
			let peer = net.peer(*peer_id);
			let client = peer.client().as_full().expect("full clients are created").clone();
			let select_chain = peer.select_chain().expect("full client has a select chain");
			let keystore_path = tempfile::tempdir().expect("Creates keystore path");
			let keystore = sc_keystore::Store::open(keystore_path.path(), None).expect("Creates keystore.");

			keystore.write().insert_ephemeral_from_seed::<AuthorityPair>(&key.to_seed())
				.expect("Creates authority key");
			keystore_paths.push(keystore_path);

			let environ = DummyFactory(client.clone());
			import_notifications.push(
				client.import_notification_stream()
					.take_while(|n| future::ready(!(n.origin != BlockOrigin::Own && n.header.number() < &5)))
					.for_each(move |_| future::ready(()))
			);

			let slot_duration = SlotDuration::get_or_compute(&*client)
				.expect("slot duration available");

			let inherent_data_providers = InherentDataProviders::new();
			register_aura_inherent_data_provider(
				&inherent_data_providers, slot_duration.get()
			).expect("Registers aura inherent data provider");

			let aura = start_aura::<_, _, _, _, _, AuthorityPair, _, _, _, _>(
				slot_duration,
				client.clone(),
				select_chain,
				client,
				environ,
				DummyOracle,
				inherent_data_providers,
				false,
				keystore,
				sp_consensus::AlwaysCanAuthor,
			).expect("Starts aura");

			runtime.spawn(aura);
		}

		runtime.spawn(futures01::future::poll_fn(move || {
			net.lock().poll();
			Ok::<_, ()>(futures01::Async::NotReady::<()>)
		}));

		runtime.block_on(future::join_all(import_notifications)
			.map(|_| Ok::<(), ()>(())).compat()).unwrap();
	}

	#[test]
	fn authorities_call_works() {
		let client = substrate_test_runtime_client::new();

		assert_eq!(client.chain_info().best_number, 0);
		assert_eq!(authorities(&client, &BlockId::Number(0)).unwrap(), vec![
			Keyring::Alice.public().into(),
			Keyring::Bob.public().into(),
			Keyring::Charlie.public().into()
		]);
	}
}
