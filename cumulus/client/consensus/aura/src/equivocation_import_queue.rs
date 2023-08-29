// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

/// An import queue which provides some equivocation resistance with lenient trait bounds.
///
/// Equivocation resistance in general is a hard problem, as different nodes in the network
/// may see equivocations in a different order, and therefore may not agree on which blocks
/// should be thrown out and which ones should be kept.
use codec::Codec;
use cumulus_client_consensus_common::ParachainBlockImportMarker;
use schnellru::{ByLength, LruMap};

use sc_consensus::{
	import_queue::{BasicQueue, Verifier as VerifierT},
	BlockImport, BlockImportParams, ForkChoiceStrategy,
};
use sc_consensus_aura::standalone as aura_internal;
use sc_telemetry::{telemetry, TelemetryHandle, CONSENSUS_DEBUG, CONSENSUS_TRACE};
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_consensus::error::Error as ConsensusError;
use sp_consensus_aura::{AuraApi, Slot, SlotDuration};
use sp_core::crypto::Pair;
use sp_inherents::{CreateInherentDataProviders, InherentDataProvider};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::{fmt::Debug, sync::Arc};

const LRU_WINDOW: u32 = 256;
const EQUIVOCATION_LIMIT: usize = 16;

struct NaiveEquivocationDefender {
	cache: LruMap<u64, usize>,
}

impl Default for NaiveEquivocationDefender {
	fn default() -> Self {
		NaiveEquivocationDefender { cache: LruMap::new(ByLength::new(LRU_WINDOW)) }
	}
}

impl NaiveEquivocationDefender {
	// return `true` if equivocation is beyond the limit.
	fn insert_and_check(&mut self, slot: Slot) -> bool {
		let val = self
			.cache
			.get_or_insert(*slot, || 0)
			.expect("insertion with ByLength limiter always succeeds; qed");
		if *val == EQUIVOCATION_LIMIT {
			true
		} else {
			*val += 1;
			false
		}
	}
}

struct Verifier<P, Client, Block, CIDP> {
	client: Arc<Client>,
	create_inherent_data_providers: CIDP,
	slot_duration: SlotDuration,
	defender: NaiveEquivocationDefender,
	telemetry: Option<TelemetryHandle>,
	_phantom: std::marker::PhantomData<fn() -> (Block, P)>,
}

#[async_trait::async_trait]
impl<P, Client, Block, CIDP> VerifierT<Block> for Verifier<P, Client, Block, CIDP>
where
	P: Pair,
	P::Signature: Codec,
	P::Public: Codec + Debug,
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + Send + Sync,
	<Client as ProvideRuntimeApi<Block>>::Api: BlockBuilderApi<Block> + AuraApi<Block, P::Public>,

	CIDP: CreateInherentDataProviders<Block, ()>,
{
	async fn verify(
		&mut self,
		mut block_params: BlockImportParams<Block>,
	) -> Result<BlockImportParams<Block>, String> {
		// Skip checks that include execution, if being told so, or when importing only state.
		//
		// This is done for example when gap syncing and it is expected that the block after the gap
		// was checked/chosen properly, e.g. by warp syncing to this block using a finality proof.
		if block_params.state_action.skip_execution_checks() || block_params.with_state() {
			return Ok(block_params)
		}

		let post_hash = block_params.header.hash();
		let parent_hash = *block_params.header.parent_hash();

		// check seal and update pre-hash/post-hash
		{
			let authorities = aura_internal::fetch_authorities(self.client.as_ref(), parent_hash)
				.map_err(|e| {
				format!("Could not fetch authorities at {:?}: {}", parent_hash, e)
			})?;

			let slot_now = slot_now(self.slot_duration);
			let res = aura_internal::check_header_slot_and_seal::<Block, P>(
				slot_now,
				block_params.header,
				&authorities,
			);

			match res {
				Ok((pre_header, slot, seal_digest)) => {
					telemetry!(
						self.telemetry;
						CONSENSUS_TRACE;
						"aura.checked_and_importing";
						"pre_header" => ?pre_header,
					);

					block_params.header = pre_header;
					block_params.post_digests.push(seal_digest);
					block_params.fork_choice = Some(ForkChoiceStrategy::LongestChain);
					block_params.post_hash = Some(post_hash);

					// Check for and reject egregious amounts of equivocations.
					if self.defender.insert_and_check(slot) {
						return Err(format!(
							"Rejecting block {:?} due to excessive equivocations at slot",
							post_hash,
						))
					}
				},
				Err(aura_internal::SealVerificationError::Deferred(hdr, slot)) => {
					telemetry!(
						self.telemetry;
						CONSENSUS_DEBUG;
						"aura.header_too_far_in_future";
						"hash" => ?post_hash,
						"a" => ?hdr,
						"b" => ?slot,
					);

					return Err(format!(
						"Rejecting block ({:?}) from future slot {:?}",
						post_hash, slot
					))
				},
				Err(e) =>
					return Err(format!(
						"Rejecting block ({:?}) with invalid seal ({:?})",
						post_hash, e
					)),
			}
		}

		// check inherents.
		if let Some(body) = block_params.body.clone() {
			let block = Block::new(block_params.header.clone(), body);
			let create_inherent_data_providers = self
				.create_inherent_data_providers
				.create_inherent_data_providers(parent_hash, ())
				.await
				.map_err(|e| format!("Could not create inherent data {:?}", e))?;

			let inherent_data = create_inherent_data_providers
				.create_inherent_data()
				.await
				.map_err(|e| format!("Could not create inherent data {:?}", e))?;

			let inherent_res = self
				.client
				.runtime_api()
				.check_inherents(parent_hash, block, inherent_data)
				.map_err(|e| format!("Unable to check block inherents {:?}", e))?;

			if !inherent_res.ok() {
				for (i, e) in inherent_res.into_errors() {
					match create_inherent_data_providers.try_handle_error(&i, &e).await {
						Some(res) => res.map_err(|e| format!("Inherent Error {:?}", e))?,
						None =>
							return Err(format!(
								"Unknown inherent error, source {:?}",
								String::from_utf8_lossy(&i[..])
							)),
					}
				}
			}
		}

		Ok(block_params)
	}
}

fn slot_now(slot_duration: SlotDuration) -> Slot {
	let timestamp = sp_timestamp::InherentDataProvider::from_system_time().timestamp();
	Slot::from_timestamp(timestamp, slot_duration)
}

/// Start an import queue for a Cumulus node which checks blocks' seals and inherent data.
///
/// Pass in only inherent data providers which don't include aura or parachain consensus inherents,
/// e.g. things like timestamp and custom inherents for the runtime.
///
/// The others are generated explicitly internally.
///
/// This should only be used for runtimes where the runtime does not check all inherents and
/// seals in `execute_block` (see <https://github.com/paritytech/cumulus/issues/2436>)
pub fn fully_verifying_import_queue<P, Client, Block: BlockT, I, CIDP>(
	client: Arc<Client>,
	block_import: I,
	create_inherent_data_providers: CIDP,
	slot_duration: SlotDuration,
	spawner: &impl sp_core::traits::SpawnEssentialNamed,
	registry: Option<&substrate_prometheus_endpoint::Registry>,
	telemetry: Option<TelemetryHandle>,
) -> BasicQueue<Block>
where
	P: Pair + 'static,
	P::Signature: Codec,
	P::Public: Codec + Debug,
	I: BlockImport<Block, Error = ConsensusError>
		+ ParachainBlockImportMarker
		+ Send
		+ Sync
		+ 'static,
	Client: ProvideRuntimeApi<Block> + Send + Sync + 'static,
	<Client as ProvideRuntimeApi<Block>>::Api: BlockBuilderApi<Block> + AuraApi<Block, P::Public>,
	CIDP: CreateInherentDataProviders<Block, ()> + 'static,
{
	let verifier = Verifier::<P, _, _, _> {
		client,
		create_inherent_data_providers,
		defender: NaiveEquivocationDefender::default(),
		slot_duration,
		telemetry,
		_phantom: std::marker::PhantomData,
	};

	BasicQueue::new(verifier, Box::new(block_import), None, spawner, registry)
}
