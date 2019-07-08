// Copyright 2019 Parity Technologies (UK) Ltd.
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

//! # BABE consensus
//!
//! BABE (Blind Assignment for Blockchain Extension) consensus in Substrate.
//!
//! # Stability
//!
//! This crate is highly unstable and experimental.  Breaking changes may
//! happen at any point.  This crate is also missing features, such as banning
//! of malicious validators, that are essential for a production network.
#![forbid(unsafe_code, missing_docs, unused_must_use)]
#![cfg_attr(not(test), forbid(dead_code))]
extern crate core;
mod digest;
use digest::CompatibleDigestItem;
pub use digest::{BabePreDigest, BABE_VRF_PREFIX};
pub use babe_primitives::*;
pub use consensus_common::SyncOracle;
use consensus_common::import_queue::{
	SharedBlockImport, SharedJustificationImport, SharedFinalityProofImport,
	SharedFinalityProofRequestBuilder,
};
use consensus_common::well_known_cache_keys::Id as CacheKeyId;
use runtime_primitives::{generic, generic::{BlockId, OpaqueDigestItemId}, Justification};
use runtime_primitives::traits::{
	Block, Header, DigestItemFor, ProvideRuntimeApi,
	SimpleBitOps, Zero,
};
use std::{sync::Arc, u64, fmt::{Debug, Display}, time::{Instant, Duration}};
use runtime_support::serde::{Serialize, Deserialize};
use parity_codec::{Decode, Encode};
use parking_lot::Mutex;
use primitives::{Pair, Public, sr25519};
use merlin::Transcript;
use inherents::{InherentDataProviders, InherentData};
use substrate_telemetry::{
	telemetry,
	CONSENSUS_TRACE,
	CONSENSUS_DEBUG,
	CONSENSUS_WARN,
	CONSENSUS_INFO,
};
use schnorrkel::{
	keys::Keypair,
	vrf::{
		VRFProof, VRFProofBatchable, VRFInOut,
	},
};
use consensus_common::{
	self, BlockImport, Environment, Proposer,
	ForkChoiceStrategy, ImportBlock, BlockOrigin, Error as ConsensusError,
};
use srml_babe::{
	BabeInherentData,
	timestamp::{TimestampInherentData, InherentType as TimestampInherent}
};
use consensus_common::{SelectChain, well_known_cache_keys};
use consensus_common::import_queue::{Verifier, BasicQueue};
use client::{
	block_builder::api::BlockBuilder as BlockBuilderApi,
	blockchain::ProvideCache,
	runtime_api::ApiExt,
	error::Result as CResult,
	backend::AuxStore,
};
use slots::{CheckedHeader, check_equivocation};
use futures::{Future, IntoFuture, future};
use tokio_timer::Timeout;
use log::{error, warn, debug, info, trace};

use slots::{SlotWorker, SlotData, SlotInfo, SlotCompatible, SignedDuration};

pub use babe_primitives::AuthorityId;

/// A slot duration. Create with `get_or_compute`.
// FIXME: Once Rust has higher-kinded types, the duplication between this
// and `super::babe::Config` can be eliminated.
// https://github.com/paritytech/substrate/issues/2434
pub struct Config(slots::SlotDuration<BabeConfiguration>);

impl Config {
	/// Either fetch the slot duration from disk or compute it from the genesis
	/// state.
	pub fn get_or_compute<B: Block, C>(client: &C) -> CResult<Self>
	where
		C: AuxStore + ProvideRuntimeApi, C::Api: BabeApi<B>,
	{
		trace!(target: "babe", "Getting slot duration");
		match slots::SlotDuration::get_or_compute(client, |a, b| a.startup_data(b)).map(Self) {
			Ok(s) => Ok(s),
			Err(s) => {
				warn!(target: "babe", "Failed to get slot duration");
				Err(s)
			}
		}
	}

	/// Get the slot duration in milliseconds.
	pub fn get(&self) -> u64 {
		self.0.slot_duration
	}

	/// Retrieve the threshold for BABE
	pub fn threshold(&self) -> u64 {
		self.0.threshold
	}
}

impl SlotCompatible for BabeLink {
	fn extract_timestamp_and_slot(
		&self,
		data: &InherentData,
	) -> Result<(TimestampInherent, u64, std::time::Duration), consensus_common::Error> {
		trace!(target: "babe", "extract timestamp");
		data.timestamp_inherent_data()
			.and_then(|t| data.babe_inherent_data().map(|a| (t, a)))
			.map_err(Into::into)
			.map_err(consensus_common::Error::InherentData)
			.map(|(x, y)| (x, y, self.0.lock().0.take().unwrap_or_default()))
	}
}

/// Parameters for BABE.
pub struct BabeParams<C, E, I, SO, SC> {

	/// The configuration for BABE.  Includes the slot duration, threshold, and
	/// other parameters.
	pub config: Config,

	/// The key of the node we are running on.
	pub local_key: Arc<sr25519::Pair>,

	/// The client to use
	pub client: Arc<C>,

	/// The SelectChain Strategy
	pub select_chain: SC,

	/// A block importer
	pub block_import: Arc<I>,

	/// The environment
	pub env: Arc<E>,

	/// A sync oracle
	pub sync_oracle: SO,

	/// Providers for inherent data.
	pub inherent_data_providers: InherentDataProviders,

	/// Force authoring of blocks even if we are offline
	pub force_authoring: bool,

	/// The source of timestamps for relative slots
	pub time_source: BabeLink,
}

/// Start the babe worker. The returned future should be run in a tokio runtime.
pub fn start_babe<B, C, SC, E, I, SO, Error, H>(BabeParams {
	config,
	local_key,
	client,
	select_chain,
	block_import,
	env,
	sync_oracle,
	inherent_data_providers,
	force_authoring,
	time_source,
}: BabeParams<C, E, I, SO, SC>) -> Result<
	impl Future<Item=(), Error=()>,
	consensus_common::Error,
> where
	B: Block<Header=H>,
	C: ProvideRuntimeApi + ProvideCache<B>,
	C::Api: BabeApi<B>,
	SC: SelectChain<B>,
	E::Proposer: Proposer<B, Error=Error>,
	<<E::Proposer as Proposer<B>>::Create as IntoFuture>::Future: Send + 'static,
	H: Header<Hash=B::Hash>,
	E: Environment<B, Error=Error>,
	I: BlockImport<B> + Send + Sync + 'static,
	Error: std::error::Error + Send + From<::consensus_common::Error> + From<I::Error> + 'static,
	SO: SyncOracle + Send + Sync + Clone,
{
	let worker = BabeWorker {
		client: client.clone(),
		block_import,
		env,
		local_key,
		sync_oracle: sync_oracle.clone(),
		force_authoring,
		threshold: config.threshold(),
	};
	register_babe_inherent_data_provider(&inherent_data_providers, config.0.slot_duration())?;
	Ok(slots::start_slot_worker::<_, _, _, _, _, _>(
		config.0,
		select_chain,
		worker,
		sync_oracle,
		inherent_data_providers,
		time_source,
	))
}

struct BabeWorker<C, E, I, SO> {
	client: Arc<C>,
	block_import: Arc<I>,
	env: Arc<E>,
	local_key: Arc<sr25519::Pair>,
	sync_oracle: SO,
	force_authoring: bool,
	threshold: u64,
}

impl<Hash, H, B, C, E, I, Error, SO> SlotWorker<B> for BabeWorker<C, E, I, SO> where
	B: Block<Header=H, Hash=Hash>,
	C: ProvideRuntimeApi + ProvideCache<B>,
	C::Api: BabeApi<B>,
	E: Environment<B, Error=Error>,
	E::Proposer: Proposer<B, Error=Error>,
	<<E::Proposer as Proposer<B>>::Create as IntoFuture>::Future: Send + 'static,
	Hash: Debug + Eq + Copy + SimpleBitOps + Encode + Decode + Serialize +
		for<'de> Deserialize<'de> + Debug + Default + AsRef<[u8]> + AsMut<[u8]> +
		std::hash::Hash + Display + Send + Sync + 'static,
	H: Header<Hash=B::Hash>,
	I: BlockImport<B> + Send + Sync + 'static,
	SO: SyncOracle + Send + Clone,
	Error: std::error::Error + Send + From<::consensus_common::Error> + From<I::Error> + 'static,
{
	type OnSlot = Box<dyn Future<Item=(), Error=consensus_common::Error> + Send>;

	fn on_slot(
		&self,
		chain_head: B::Header,
		slot_info: SlotInfo,
	) -> Self::OnSlot {
		let pair = self.local_key.clone();
		let ref client = self.client;
		let block_import = self.block_import.clone();
		let ref env = self.env;

		let (timestamp, slot_num, slot_duration) =
			(slot_info.timestamp, slot_info.number, slot_info.duration);

		let authorities = match authorities(client.as_ref(), &BlockId::Hash(chain_head.hash())) {
			Ok(authorities) => authorities,
			Err(e) => {
				error!(
					target: "babe",
					"Unable to fetch authorities at block {:?}: {:?}",
					chain_head.hash(),
					e
				);
				telemetry!(CONSENSUS_WARN; "babe.unable_fetching_authorities";
					"slot" => ?chain_head.hash(), "err" => ?e
				);
				return Box::new(future::ok(()));
			}
		};

		if !self.force_authoring && self.sync_oracle.is_offline() && authorities.len() > 1 {
			debug!(target: "babe", "Skipping proposal slot. Waiting for the network.");
			telemetry!(CONSENSUS_DEBUG; "babe.skipping_proposal_slot";
				"authorities_len" => authorities.len()
			);
			return Box::new(future::ok(()));
		}

		// FIXME replace the dummy empty slices with real data
		// https://github.com/paritytech/substrate/issues/2435
		// https://github.com/paritytech/substrate/issues/2436
		let proposal_work = if let Some(((inout, proof, _batchable_proof), index)) = claim_slot(
			&[0u8; 0],
			slot_info.number,
			&[0u8; 0],
			0,
			&authorities,
			&pair,
			self.threshold,
		) {
			debug!(
				target: "babe", "Starting authorship at slot {}; timestamp = {}",
				slot_num,
				timestamp,
			);
			telemetry!(CONSENSUS_DEBUG; "babe.starting_authorship";
				"slot_num" => slot_num, "timestamp" => timestamp
			);

			// we are the slot author. make a block and sign it.
			let proposer = match env.init(&chain_head) {
				Ok(p) => p,
				Err(e) => {
					warn!(target: "babe", "Unable to author block in slot {:?}: {:?}", slot_num, e);
					telemetry!(CONSENSUS_WARN; "babe.unable_authoring_block";
						"slot" => slot_num, "err" => ?e
					);
					return Box::new(future::ok(()))
				}
			};

			let inherent_digest = BabePreDigest {
				proof,
				vrf_output: inout.to_output(),
				index: index as u64,
				slot_num,
			};

			// deadline our production to approx. the end of the slot
			let remaining_duration = slot_info.remaining_duration();
			Timeout::new(
				proposer.propose(
					slot_info.inherent_data,
					generic::Digest {
						logs: vec![
							generic::DigestItem::babe_pre_digest(inherent_digest.clone()),
						],
					},
					remaining_duration,
				).into_future(),
				remaining_duration,
			)
		} else {
			return Box::new(future::ok(()));
		};

		Box::new(proposal_work.map(move |b| {
			// minor hack since we don't have access to the timestamp
			// that is actually set by the proposer.
			let slot_after_building = SignedDuration::default().slot_now(slot_duration);
			if slot_after_building != slot_num {
				info!(
					target: "babe",
					"Discarding proposal for slot {}; block production took too long",
					slot_num
				);
				telemetry!(CONSENSUS_INFO; "babe.discarding_proposal_took_too_long";
					"slot" => slot_num
				);
				return
			}

			let (header, body) = b.deconstruct();
			let pre_digest: Result<BabePreDigest, String> = find_pre_digest::<B>(&header);
			if let Err(e) = pre_digest {
				error!(target: "babe", "FATAL ERROR: Invalid pre-digest: {}!", e);
				return
			} else {
				trace!(target: "babe", "Got correct number of seals.  Good!")
			};

			let header_num = header.number().clone();
			let parent_hash = header.parent_hash().clone();

			// sign the pre-sealed hash of the block and then
			// add it to a digest item.
			let header_hash = header.hash();
			let signature = pair.sign(header_hash.as_ref());
			let signature_digest_item = DigestItemFor::<B>::babe_seal(signature);

			let import_block: ImportBlock<B> = ImportBlock {
				origin: BlockOrigin::Own,
				header,
				justification: None,
				post_digests: vec![signature_digest_item],
				body: Some(body),
				finalized: false,
				auxiliary: Vec::new(),
				fork_choice: ForkChoiceStrategy::LongestChain,
			};

			info!(target: "babe",
					"Pre-sealed block for proposal at {}. Hash now {:?}, previously {:?}.",
					header_num,
					import_block.post_header().hash(),
					header_hash,
			);
			telemetry!(CONSENSUS_INFO; "babe.pre_sealed_block";
				"header_num" => ?header_num,
				"hash_now" => ?import_block.post_header().hash(),
				"hash_previously" => ?header_hash,
			);

			if let Err(e) = block_import.import_block(import_block, Default::default()) {
				warn!(target: "babe", "Error with block built on {:?}: {:?}",
						parent_hash, e);
				telemetry!(CONSENSUS_WARN; "babe.err_with_block_built_on";
					"hash" => ?parent_hash, "err" => ?e
				);
			}
		}).map_err(|e| {
			warn!("Client import failed: {:?}", e);
			consensus_common::Error::ClientImport(format!("{:?}", e)).into()
		}))
	}
}

macro_rules! babe_err {
	($($i: expr),+) => {
		{ debug!(target: "babe", $($i),+)
		; format!($($i),+)
		}
	};
}

fn find_pre_digest<B: Block>(header: &B::Header) -> Result<BabePreDigest, String>
	where DigestItemFor<B>: CompatibleDigestItem,
{
	let mut pre_digest: Option<_> = None;
	for log in header.digest().logs() {
		trace!(target: "babe", "Checking log {:?}", log);
		match (log.as_babe_pre_digest(), pre_digest.is_some()) {
			(Some(_), true) => Err(babe_err!("Multiple BABE pre-runtime headers, rejecting!"))?,
			(None, _) => trace!(target: "babe", "Ignoring digest not meant for us"),
			(s, false) => pre_digest = s,
		}
	}
	pre_digest.ok_or_else(|| babe_err!("No BABE pre-runtime digest found"))
}

/// check a header has been signed by the right key. If the slot is too far in
/// the future, an error will be returned. If successful, returns the pre-header
/// and the digest item containing the seal.
///
/// The seal must be the last digest.  Otherwise, the whole header is considered
/// unsigned.  This is required for security and must not be changed.
///
/// This digest item will always return `Some` when used with `as_babe_pre_digest`.
//
// FIXME #1018 needs misbehavior types
fn check_header<B: Block + Sized, C: AuxStore>(
	client: &C,
	slot_now: u64,
	mut header: B::Header,
	hash: B::Hash,
	authorities: &[AuthorityId],
	threshold: u64,
) -> Result<CheckedHeader<B::Header, (DigestItemFor<B>, DigestItemFor<B>)>, String>
	where DigestItemFor<B>: CompatibleDigestItem,
{
	trace!(target: "babe", "Checking header");
	let seal = match header.digest_mut().pop() {
		Some(x) => x,
		None => return Err(babe_err!("Header {:?} is unsealed", hash)),
	};

	let sig = seal.as_babe_seal().ok_or_else(|| {
		babe_err!("Header {:?} has a bad seal", hash)
	})?;

	let pre_digest = find_pre_digest::<B>(&header)?;
	let BabePreDigest { slot_num, index, ref proof, ref vrf_output } = pre_digest;

	if slot_num > slot_now {
		header.digest_mut().push(seal);
		Ok(CheckedHeader::Deferred(header, slot_num))
	} else if index > authorities.len() as u64 {
		Err(babe_err!("Slot author not found"))
	} else {
		let (pre_hash, author): (_, &sr25519::Public) = (header.hash(), &authorities[index as usize]);

		if sr25519::Pair::verify(&sig, pre_hash, author.clone()) {
			let (inout, _batchable_proof) = {
				let transcript = make_transcript(
					Default::default(),
					slot_num,
					Default::default(),
					0,
				);
				schnorrkel::PublicKey::from_bytes(author.as_slice()).and_then(|p| {
					p.vrf_verify(transcript, vrf_output, proof)
				}).map_err(|s| {
					babe_err!("VRF verification failed: {:?}", s)
				})?
			};

			if !check(&inout, threshold) {
				return Err(babe_err!("VRF verification of block by author {:?} failed: \
									  threshold {} exceeded", author, threshold));
			}

			if let Some(equivocation_proof) = check_equivocation(
				client,
				slot_now,
				slot_num,
				&header,
				author,
			).map_err(|e| e.to_string())? {
				info!(
					"Slot author {:?} is equivocating at slot {} with headers {:?} and {:?}",
					author,
					slot_num,
					equivocation_proof.fst_header().hash(),
					equivocation_proof.snd_header().hash(),
				);
			}

			let pre_digest = CompatibleDigestItem::babe_pre_digest(pre_digest);
			Ok(CheckedHeader::Checked(header, (pre_digest, seal)))
		} else {
			Err(babe_err!("Bad signature on {:?}", hash))
		}
	}
}

/// State that must be shared between the import queue and the authoring logic.
#[derive(Default, Clone, Debug)]
pub struct BabeLink(Arc<Mutex<(Option<Duration>, Vec<(Instant, u64)>)>>);

/// A verifier for Babe blocks.
pub struct BabeVerifier<C> {
	client: Arc<C>,
	inherent_data_providers: inherents::InherentDataProviders,
	config: Config,
	time_source: BabeLink,
}

impl<C> BabeVerifier<C> {
	fn check_inherents<B: Block>(
		&self,
		block: B,
		block_id: BlockId<B>,
		inherent_data: InherentData,
	) -> Result<(), String>
		where C: ProvideRuntimeApi, C::Api: BlockBuilderApi<B>
	{
		let inherent_res = self.client.runtime_api().check_inherents(
			&block_id,
			block,
			inherent_data,
		).map_err(|e| format!("{:?}", e))?;

		if !inherent_res.ok() {
			inherent_res
				.into_errors()
				.try_for_each(|(i, e)| Err(self.inherent_data_providers.error_to_string(&i, &e)))
		} else {
			Ok(())
		}
	}
}

fn median_algorithm(
	median_required_blocks: u64,
	slot_duration: u64,
	slot_num: u64,
	slot_now: u64,
	time_source: &mut (Option<Duration>, Vec<(Instant, u64)>),
) {
	let num_timestamps = time_source.1.len();
	if num_timestamps as u64 >= median_required_blocks && median_required_blocks > 0 {
		let mut new_list: Vec<_> = time_source.1.iter().map(|&(t, sl)| {
				let offset: u128 = u128::from(slot_duration)
					.checked_mul(1_000_000u128) // self.config.get() returns *milliseconds*
					.and_then(|x| x.checked_mul(u128::from(slot_num).saturating_sub(u128::from(sl))))
					.expect("we cannot have timespans long enough for this to overflow; qed");
				const NANOS_PER_SEC: u32 = 1_000_000_000;
				let nanos = (offset % u128::from(NANOS_PER_SEC)) as u32;
				let secs = (offset / u128::from(NANOS_PER_SEC)) as u64;
				t + Duration::new(secs, nanos)
			}).collect();
		// FIXME #2926: use a selection algorithm instead of a full sorting algorithm.
		new_list.sort_unstable();
		let &median = new_list
			.get(num_timestamps / 2)
			.expect("we have at least one timestamp, so this is a valid index; qed");
		time_source.1.clear();
		// FIXME #2927: pass this to the block authoring logic somehow
		time_source.0.replace(Instant::now() - median);
	} else {
		time_source.1.push((Instant::now(), slot_now))
	}
}

impl<B: Block, C> Verifier<B> for BabeVerifier<C> where
	C: ProvideRuntimeApi + Send + Sync + AuxStore + ProvideCache<B>,
	C::Api: BlockBuilderApi<B> + BabeApi<B>,
	DigestItemFor<B>: CompatibleDigestItem,
{
	fn verify(
		&self,
		origin: BlockOrigin,
		header: B::Header,
		justification: Option<Justification>,
		mut body: Option<Vec<B::Extrinsic>>,
	) -> Result<(ImportBlock<B>, Option<Vec<(CacheKeyId, Vec<u8>)>>), String> {
		trace!(
			target: "babe",
			"Verifying origin: {:?} header: {:?} justification: {:?} body: {:?}",
			origin,
			header,
			justification,
			body,
		);

		debug!(target: "babe", "We have {:?} logs in this header", header.digest().logs().len());
		let mut inherent_data = self
			.inherent_data_providers
			.create_inherent_data()
			.map_err(String::from)?;
		let (_, slot_now, _) = self.time_source.extract_timestamp_and_slot(&inherent_data)
			.map_err(|e| format!("Could not extract timestamp and slot: {:?}", e))?;
		let hash = header.hash();
		let parent_hash = *header.parent_hash();
		let authorities = authorities(self.client.as_ref(), &BlockId::Hash(parent_hash))
			.map_err(|e| format!("Could not fetch authorities at {:?}: {:?}", parent_hash, e))?;

		// we add one to allow for some small drift.
		// FIXME #1019 in the future, alter this queue to allow deferring of
		// headers
		let checked_header = check_header::<B, C>(
			&self.client,
			slot_now + 1,
			header,
			hash,
			&authorities[..],
			self.config.threshold(),
		)?;
		match checked_header {
			CheckedHeader::Checked(pre_header, (pre_digest, seal)) => {
				let BabePreDigest { slot_num, .. } = pre_digest.as_babe_pre_digest()
					.expect("check_header always returns a pre-digest digest item; qed");

				// if the body is passed through, we need to use the runtime
				// to check that the internally-set timestamp in the inherents
				// actually matches the slot set in the seal.
				if let Some(inner_body) = body.take() {
					inherent_data.babe_replace_inherent_data(slot_num);
					let block = B::new(pre_header.clone(), inner_body);

					self.check_inherents(
						block.clone(),
						BlockId::Hash(parent_hash),
						inherent_data,
					)?;

					let (_, inner_body) = block.deconstruct();
					body = Some(inner_body);
				}

				trace!(target: "babe", "Checked {:?}; importing.", pre_header);
				telemetry!(
					CONSENSUS_TRACE;
					"babe.checked_and_importing";
					"pre_header" => ?pre_header);

				// `Consensus` is the Babe-specific authorities change log.
				// It's an encoded `Vec<AuthorityId>`, the same format as is stored in the cache,
				// so no need to decode/re-encode.
				let maybe_keys = pre_header.digest()
					.log(|l| l.try_as_raw(OpaqueDigestItemId::Consensus(&BABE_ENGINE_ID)))
					.map(|blob| vec![(well_known_cache_keys::AUTHORITIES, blob.to_vec())]);

				let import_block = ImportBlock {
					origin,
					header: pre_header,
					post_digests: vec![seal],
					body,
					finalized: false,
					justification,
					auxiliary: Vec::new(),
					fork_choice: ForkChoiceStrategy::LongestChain,
				};
				median_algorithm(
					self.config.0.median_required_blocks,
					self.config.get(),
					slot_num,
					slot_now,
					&mut *self.time_source.0.lock(),
				);
				// FIXME #1019 extract authorities
				Ok((import_block, maybe_keys))
			}
			CheckedHeader::Deferred(a, b) => {
				debug!(target: "babe", "Checking {:?} failed; {:?}, {:?}.", hash, a, b);
				telemetry!(CONSENSUS_DEBUG; "babe.header_too_far_in_future";
					"hash" => ?hash, "a" => ?a, "b" => ?b
				);
				Err(format!("Header {:?} rejected: too far in the future", hash))
			}
		}
	}
}

fn authorities<B, C>(client: &C, at: &BlockId<B>) -> Result<
	Vec<AuthorityId>,
	ConsensusError,
> where
	B: Block,
	C: ProvideRuntimeApi + ProvideCache<B>,
	C::Api: BabeApi<B>,
{
	client
		.cache()
		.and_then(|cache| cache.get_at(&well_known_cache_keys::AUTHORITIES, at)
			.and_then(|v| Decode::decode(&mut &v[..])))
		.or_else(|| {
			if client.runtime_api().has_api::<dyn BabeApi<B>>(at).unwrap_or(false) {
				BabeApi::authorities(&*client.runtime_api(), at).ok()
			} else {
				panic!("We don’t support deprecated code with new consensus algorithms, \
						therefore this is unreachable; qed")
			}
		}).ok_or(consensus_common::Error::InvalidAuthoritiesSet)
}

/// The BABE import queue type.
pub type BabeImportQueue<B> = BasicQueue<B>;

/// Register the babe inherent data provider, if not registered already.
fn register_babe_inherent_data_provider(
	inherent_data_providers: &InherentDataProviders,
	slot_duration: u64,
) -> Result<(), consensus_common::Error> {
	debug!(target: "babe", "Registering");
	if !inherent_data_providers.has_provider(&srml_babe::INHERENT_IDENTIFIER) {
		inherent_data_providers
			.register_provider(srml_babe::InherentDataProvider::new(slot_duration))
			.map_err(Into::into)
			.map_err(consensus_common::Error::InherentData)
	} else {
		Ok(())
	}
}

fn get_keypair(q: &sr25519::Pair) -> &Keypair {
	q.as_ref()
}

#[allow(deprecated)]
fn make_transcript(
	randomness: &[u8],
	slot_number: u64,
	genesis_hash: &[u8],
	epoch: u64,
) -> Transcript {
	let mut transcript = Transcript::new(&BABE_ENGINE_ID);
	transcript.commit_bytes(b"slot number", &slot_number.to_le_bytes());
	transcript.commit_bytes(b"genesis block hash", genesis_hash);
	transcript.commit_bytes(b"current epoch", &epoch.to_le_bytes());
	transcript.commit_bytes(b"chain randomness", randomness);
	transcript
}

fn check(inout: &VRFInOut, threshold: u64) -> bool {
	u64::from_le_bytes(inout.make_bytes::<[u8; 8]>(BABE_VRF_PREFIX)) < threshold
}

/// Claim a slot if it is our turn.  Returns `None` if it is not our turn.
///
/// This hashes the slot number, epoch, genesis hash, and chain randomness into
/// the VRF.  If the VRF produces a value less than `threshold`, it is our turn,
/// so it returns `Some(_)`.  Otherwise, it returns `None`.
fn claim_slot(
	randomness: &[u8],
	slot_number: u64,
	genesis_hash: &[u8],
	epoch: u64,
	authorities: &[AuthorityId],
	key: &sr25519::Pair,
	threshold: u64,
) -> Option<((VRFInOut, VRFProof, VRFProofBatchable), usize)> {
	let public = &key.public();
	let index = authorities.iter().position(|s| s == public)?;
	let transcript = make_transcript(
		randomness,
		slot_number,
		genesis_hash,
		epoch,
	);

	// Compute the threshold we will use.
	//
	// We already checked that authorities contains `key.public()`, so it can’t
	// be empty.  Therefore, this division is safe.
	let threshold = threshold / authorities.len() as u64;

	get_keypair(key)
		.vrf_sign_n_check(transcript, |inout| check(inout, threshold))
		.map(|s|(s, index))
}

fn initialize_authorities_cache<B, C>(client: &C) -> Result<(), ConsensusError> where
	B: Block,
	C: ProvideRuntimeApi + ProvideCache<B>,
	C::Api: BabeApi<B>,
{
	// no cache => no initialization
	let cache = match client.cache() {
		Some(cache) => cache,
		None => return Ok(()),
	};

	// check if we already have initialized the cache
	let genesis_id = BlockId::Number(Zero::zero());
	let genesis_authorities: Option<Vec<AuthorityId>> = cache
		.get_at(&well_known_cache_keys::AUTHORITIES, &genesis_id)
		.and_then(|v| Decode::decode(&mut &v[..]));
	if genesis_authorities.is_some() {
		return Ok(());
	}

	let map_err = |error| consensus_common::Error::from(consensus_common::Error::ClientImport(
		format!(
			"Error initializing authorities cache: {}",
			error,
		)));
	let genesis_authorities = authorities(client, &genesis_id)?;
	cache.initialize(&well_known_cache_keys::AUTHORITIES, genesis_authorities.encode())
		.map_err(map_err)
}

/// Start an import queue for the Babe consensus algorithm.
pub fn import_queue<B, C, E>(
	config: Config,
	block_import: SharedBlockImport<B>,
	justification_import: Option<SharedJustificationImport<B>>,
	finality_proof_import: Option<SharedFinalityProofImport<B>>,
	finality_proof_request_builder: Option<SharedFinalityProofRequestBuilder<B>>,
	client: Arc<C>,
	inherent_data_providers: InherentDataProviders,
) -> Result<(BabeImportQueue<B>, BabeLink), consensus_common::Error> where
	B: Block,
	C: 'static + ProvideRuntimeApi + ProvideCache<B> + Send + Sync + AuxStore,
	C::Api: BlockBuilderApi<B> + BabeApi<B>,
	DigestItemFor<B>: CompatibleDigestItem,
	E: 'static,
{
	register_babe_inherent_data_provider(&inherent_data_providers, config.get())?;
	initialize_authorities_cache(&*client)?;

	let verifier = BabeVerifier {
		client: client,
		inherent_data_providers,
		time_source: Default::default(),
		config,
	};
	let timestamp_core = verifier.time_source.clone();
	Ok((BasicQueue::new(
		Arc::new(verifier),
		block_import,
		justification_import,
		finality_proof_import,
		finality_proof_request_builder,
	), timestamp_core))
}

// FIXME #2532: need to allow deprecated until refactor is done
// https://github.com/paritytech/substrate/issues/2532
#[cfg(test)]
#[allow(unused_imports, deprecated)]
#[cfg_attr(test, allow(dead_code))]
mod tests {
	use super::*;

	use client::LongestChain;
	use consensus_common::NoNetwork as DummyOracle;
	use network::test::*;
	use network::test::{Block as TestBlock, PeersClient};
	use runtime_primitives::traits::{Block as BlockT, DigestFor};
	use network::config::ProtocolConfig;
	use tokio::runtime::current_thread;
	use keyring::sr25519::Keyring;
	use super::generic::DigestItem;
	use client::BlockchainEvents;
	use test_client;
	use futures::{Async, stream::Stream as _};
	use log::debug;
	use std::time::Duration;
	type Item = generic::DigestItem<Hash>;
	use test_client::AuthorityKeyring;

	type Error = client::error::Error;

	type TestClient = client::Client<
		test_client::Backend,
		test_client::Executor,
		TestBlock,
		test_client::runtime::RuntimeApi,
	>;

	struct DummyFactory(Arc<TestClient>);
	struct DummyProposer(u64, Arc<TestClient>);

	impl Environment<TestBlock> for DummyFactory {
		type Proposer = DummyProposer;
		type Error = Error;

		fn init(&self, parent_header: &<TestBlock as BlockT>::Header)
			-> Result<DummyProposer, Error>
		{
			Ok(DummyProposer(parent_header.number + 1, self.0.clone()))
		}
	}

	impl Proposer<TestBlock> for DummyProposer {
		type Error = Error;
		type Create = Result<TestBlock, Error>;

		fn propose(&self, _: InherentData, digests: DigestFor<TestBlock>, _: Duration) -> Result<TestBlock, Error> {
			self.1.new_block(digests).unwrap().bake().map_err(|e| e.into())
		}
	}

	const SLOT_DURATION: u64 = 1;

	pub struct BabeTestNet {
		peers: Vec<Peer<(), DummySpecialization>>,
	}

	impl TestNetFactory for BabeTestNet {
		type Specialization = DummySpecialization;
		type Verifier = BabeVerifier<PeersFullClient>;
		type PeerData = ();

		/// Create new test network with peers and given config.
		fn from_config(_config: &ProtocolConfig) -> Self {
			debug!(target: "babe", "Creating test network from config");
			BabeTestNet {
				peers: Vec::new(),
			}
		}

		fn make_verifier(&self, client: PeersClient, _cfg: &ProtocolConfig)
			-> Arc<Self::Verifier>
		{
			let client = client.as_full().expect("only full clients are used in test");
			trace!(target: "babe", "Creating a verifier");
			let config = Config::get_or_compute(&*client)
				.expect("slot duration available");
			let inherent_data_providers = InherentDataProviders::new();
			register_babe_inherent_data_provider(
				&inherent_data_providers,
				config.get()
			).expect("Registers babe inherent data provider");
			trace!(target: "babe", "Provider registered");

			assert_eq!(config.get(), SLOT_DURATION);
			Arc::new(BabeVerifier {
				client,
				inherent_data_providers,
				config,
				time_source: Default::default(),
			})
		}

		fn peer(&mut self, i: usize) -> &mut Peer<Self::PeerData, DummySpecialization> {
			trace!(target: "babe", "Retreiving a peer");
			&mut self.peers[i]
		}

		fn peers(&self) -> &Vec<Peer<Self::PeerData, DummySpecialization>> {
			trace!(target: "babe", "Retreiving peers");
			&self.peers
		}

		fn mut_peers<F: FnOnce(&mut Vec<Peer<Self::PeerData, DummySpecialization>>)>(
			&mut self,
			closure: F,
		) {
			closure(&mut self.peers);
		}
	}

	#[test]
	fn can_serialize_block() {
		drop(env_logger::try_init());
		assert!(BabePreDigest::decode(&mut &b""[..]).is_none());
	}

	#[test]
	fn authoring_blocks() {
		drop(env_logger::try_init());
		debug!(target: "babe", "checkpoint 1");
		let net = BabeTestNet::new(3);
		debug!(target: "babe", "checkpoint 2");

		debug!(target: "babe", "checkpoint 3");

		let peers = &[
			(0, Keyring::Alice),
			(1, Keyring::Bob),
			(2, Keyring::Charlie),
		];

		let net = Arc::new(Mutex::new(net));
		let mut import_notifications = Vec::new();
		debug!(target: "babe", "checkpoint 4");
		let mut runtime = current_thread::Runtime::new().unwrap();
		for (peer_id, key) in peers {
			let client = net.lock().peer(*peer_id).client().as_full().unwrap();
			let environ = Arc::new(DummyFactory(client.clone()));
			import_notifications.push(
				client.import_notification_stream()
					.take_while(|n| Ok(!(n.origin != BlockOrigin::Own && n.header.number() < &5)))
					.for_each(move |_| Ok(()))
			);

			let config = Config::get_or_compute(&*client)
				.expect("slot duration available");

			let inherent_data_providers = InherentDataProviders::new();
			register_babe_inherent_data_provider(
				&inherent_data_providers, config.get()
			).expect("Registers babe inherent data provider");


			#[allow(deprecated)]
			let select_chain = LongestChain::new(client.backend().clone());

			runtime.spawn(start_babe(BabeParams {
				config,
				local_key: Arc::new(key.clone().into()),
				block_import: client.clone(),
				select_chain,
				client,
				env: environ.clone(),
				sync_oracle: DummyOracle,
				inherent_data_providers,
				force_authoring: false,
			    time_source: Default::default(),
			}).expect("Starts babe"));
		}
		debug!(target: "babe", "checkpoint 5");

		// wait for all finalized on each.
		let wait_for = ::futures::future::join_all(import_notifications)
			.map(drop)
			.map_err(drop);

		let drive_to_completion = futures::future::poll_fn(|| { net.lock().poll(); Ok(Async::NotReady) });
		let _ = runtime.block_on(wait_for.select(drive_to_completion).map_err(drop)).unwrap();
	}

	#[test]
	fn wrong_consensus_engine_id_rejected() {
		drop(env_logger::try_init());
		let sig = sr25519::Pair::generate().0.sign(b"");
		let bad_seal: Item = DigestItem::Seal([0; 4], sig.0.to_vec());
		assert!(bad_seal.as_babe_pre_digest().is_none());
		assert!(bad_seal.as_babe_seal().is_none())
	}

	#[test]
	fn malformed_pre_digest_rejected() {
		drop(env_logger::try_init());
		let bad_seal: Item = DigestItem::Seal(BABE_ENGINE_ID, [0; 64].to_vec());
		assert!(bad_seal.as_babe_pre_digest().is_none());
	}

	#[test]
	fn sig_is_not_pre_digest() {
		drop(env_logger::try_init());
		let sig = sr25519::Pair::generate().0.sign(b"");
		let bad_seal: Item = DigestItem::Seal(BABE_ENGINE_ID, sig.0.to_vec());
		assert!(bad_seal.as_babe_pre_digest().is_none());
		assert!(bad_seal.as_babe_seal().is_some())
	}

	#[test]
	fn can_author_block() {
		drop(env_logger::try_init());
		let randomness = &[];
		let (pair, _) = sr25519::Pair::generate();
		let mut i = 0;
		loop {
			match claim_slot(randomness, i, &[], 0, &[pair.public()], &pair, u64::MAX / 10) {
				None => i += 1,
				Some(s) => {
					debug!(target: "babe", "Authored block {:?}", s);
					break
				}
			}
		}
	}

	#[test]
	fn authorities_call_works() {
		drop(env_logger::try_init());
		let client = test_client::new();

		assert_eq!(client.info().chain.best_number, 0);
		assert_eq!(authorities(&client, &BlockId::Number(0)).unwrap(), vec![
			Keyring::Alice.into(),
			Keyring::Bob.into(),
			Keyring::Charlie.into()
		]);
	}
}
