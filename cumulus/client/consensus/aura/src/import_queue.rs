// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! Parachain specific wrapper for the AuRa import queue.

use codec::Codec;
use cumulus_client_consensus_common::ParachainBlockImportMarker;
use prometheus_endpoint::Registry;
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

/// A parachain block import verifier that checks for equivocation limits within each slot.
pub struct Verifier<P, Client, Block, CIDP> {
	client: Arc<Client>,
	create_inherent_data_providers: CIDP,
	telemetry: Option<TelemetryHandle>,
	_phantom: std::marker::PhantomData<fn() -> (Block, P)>,
}

impl<P, Client, Block, CIDP> Verifier<P, Client, Block, CIDP>
where
	P: Pair,
	P::Signature: Codec,
	P::Public: Codec + Debug,
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + Send + Sync,
	<Client as ProvideRuntimeApi<Block>>::Api: BlockBuilderApi<Block> + AuraApi<Block, P::Public>,

	CIDP: CreateInherentDataProviders<Block, ()>,
{
	/// Creates a new Verifier instance for handling parachain block import verification in Aura
	/// consensus.
	pub fn new(
		client: Arc<Client>,
		inherent_data_provider: CIDP,
		telemetry: Option<TelemetryHandle>,
	) -> Self {
		Self {
			client,
			create_inherent_data_providers: inherent_data_provider,
			telemetry,
			_phantom: std::marker::PhantomData,
		}
	}
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
		&self,
		mut block_params: BlockImportParams<Block>,
	) -> Result<BlockImportParams<Block>, String> {
		// Skip checks that include execution, if being told so, or when importing only state.
		//
		// This is done for example when gap syncing and it is expected that the block after the gap
		// was checked/chosen properly, e.g. by warp syncing to this block using a finality proof.
		if block_params.state_action.skip_execution_checks() || block_params.with_state() {
			block_params.fork_choice = Some(ForkChoiceStrategy::Custom(block_params.with_state()));
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

			let slot_duration = self
				.client
				.runtime_api()
				.slot_duration(parent_hash)
				.map_err(|e| e.to_string())?;

			let slot_now = slot_now(slot_duration);
			let res = aura_internal::check_header_slot_and_seal::<Block, P>(
				slot_now,
				block_params.header,
				&authorities,
			);

			match res {
				Ok((pre_header, _, seal_digest)) => {
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

/// Parameters for [`import_queue`].
pub struct ImportQueueParams<'a, I, C, CIDP, S> {
	/// The block import to use.
	pub block_import: I,
	/// The client to interact with the chain.
	pub client: Arc<C>,
	/// The inherent data providers, to create the inherent data.
	pub create_inherent_data_providers: CIDP,
	/// The spawner to spawn background tasks.
	pub spawner: &'a S,
	/// The prometheus registry.
	pub registry: Option<&'a Registry>,
	/// The telemetry handle.
	pub telemetry: Option<TelemetryHandle>,
}
/// Start an import queue for the Aura consensus algorithm.
pub fn import_queue<P, Block, I, C, S, CIDP>(
	ImportQueueParams {
		block_import,
		client,
		create_inherent_data_providers,
		spawner,
		registry,
		telemetry,
	}: ImportQueueParams<'_, I, C, CIDP, S>,
) -> Result<BasicQueue<Block>, sp_consensus::Error>
where
	Block: BlockT,
	C::Api: BlockBuilderApi<Block> + AuraApi<Block, P::Public>,
	C: 'static + ProvideRuntimeApi<Block> + Send + Sync,
	I: BlockImport<Block, Error = ConsensusError>
		+ ParachainBlockImportMarker
		+ Send
		+ Sync
		+ 'static,
	P: Pair + 'static,
	P::Public: Debug + Codec,
	P::Signature: Codec,
	S: sp_core::traits::SpawnEssentialNamed,
	CIDP: CreateInherentDataProviders<Block, ()> + Sync + Send + 'static,
{
	let verifier = Verifier::<P, _, _, _> {
		client,
		create_inherent_data_providers,
		telemetry,
		_phantom: std::marker::PhantomData,
	};

	Ok(BasicQueue::new(verifier, Box::new(block_import), None, spawner, registry))
}

/// Parameters of [`build_verifier`].
pub struct BuildVerifierParams<C, CIDP> {
	/// The client to interact with the chain.
	pub client: Arc<C>,
	/// The inherent data providers, to create the inherent data.
	pub create_inherent_data_providers: CIDP,
	/// The telemetry handle.
	pub telemetry: Option<TelemetryHandle>,
}

/// Build the [`Verifier`].
pub fn build_verifier<P, C, CIDP, Block>(
	BuildVerifierParams { client, create_inherent_data_providers, telemetry }: BuildVerifierParams<
		C,
		CIDP,
	>,
) -> Verifier<P, C, Block, CIDP> {
	Verifier {
		client,
		create_inherent_data_providers,
		telemetry,
		_phantom: std::marker::PhantomData,
	}
}
