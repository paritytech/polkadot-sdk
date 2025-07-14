// This file is part of Substrate.

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

//! Module implementing the logic for verifying and importing AuRa blocks.

use crate::{
	authorities, standalone::SealVerificationError, AuthorityId, CompatibilityMode, Error,
	LOG_TARGET,
};
use codec::Codec;
use log::{debug, trace};
use prometheus_endpoint::Registry;
use sc_client_api::{backend::AuxStore, BlockOf, UsageProvider};
use sc_consensus::{
	block_import::{BlockImport, BlockImportParams, ForkChoiceStrategy},
	import_queue::{BasicQueue, BoxJustificationImport, DefaultImportQueue, Verifier},
	BlockCheckParams, ImportResult,
};
use sc_consensus_slots::{check_equivocation, CheckedHeader, InherentDataProviderExt};
use sc_telemetry::{telemetry, TelemetryHandle, CONSENSUS_DEBUG, CONSENSUS_TRACE};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::HeaderBackend;
use sp_consensus::Error as ConsensusError;
use sp_consensus_aura::AuraApi;
use sp_consensus_slots::Slot;
use sp_core::crypto::Pair;
use sp_inherents::{CreateInherentDataProviders, InherentDataProvider};
use sp_runtime::{
	traits::{Block as BlockT, Header, NumberFor},
	DigestItem,
};
use std::{fmt::Debug, marker::PhantomData, sync::Arc};

/// check a header has been signed by the right key. If the slot is too far in the future, an error
/// will be returned. If it's successful, returns the pre-header and the digest item
/// containing the seal.
///
/// This digest item will always return `Some` when used with `as_aura_seal`.
fn check_header<C, B: BlockT, P: Pair>(
	slot_now: Slot,
	header: B::Header,
	hash: B::Hash,
	authorities: &[AuthorityId<P>],
) -> Result<CheckedHeader<B::Header, (Slot, DigestItem)>, Error<B>>
where
	P::Public: Codec,
	P::Signature: Codec,
	C: sc_client_api::backend::AuxStore,
{
	let check_result =
		crate::standalone::check_header_slot_and_seal::<B, P>(slot_now, header, authorities);

	match check_result {
		Ok((header, slot, seal)) => Ok(CheckedHeader::Checked(header, (slot, seal))),
		Err(SealVerificationError::Deferred(header, slot)) =>
			Ok(CheckedHeader::Deferred(header, slot)),
		Err(SealVerificationError::Unsealed) => Err(Error::HeaderUnsealed(hash)),
		Err(SealVerificationError::BadSeal) => Err(Error::HeaderBadSeal(hash)),
		Err(SealVerificationError::BadSignature) => Err(Error::BadSignature(hash)),
		Err(SealVerificationError::SlotAuthorNotFound) => Err(Error::SlotAuthorNotFound),
		Err(SealVerificationError::InvalidPreDigest(e)) => Err(Error::from(e)),
	}
}

/// A verifier for Aura blocks.
pub struct AuraVerifier<C, P, GetSlotFn, N> {
	client: Arc<C>,
	get_slot: GetSlotFn,
	telemetry: Option<TelemetryHandle>,
	compatibility_mode: CompatibilityMode<N>,
	_phantom: PhantomData<fn() -> P>,
}

impl<C, P, GetSlotFn, N> AuraVerifier<C, P, GetSlotFn, N> {
	pub(crate) fn new(
		client: Arc<C>,
		get_slot: GetSlotFn,
		telemetry: Option<TelemetryHandle>,
		compatibility_mode: CompatibilityMode<N>,
	) -> Self {
		Self { client, get_slot, telemetry, compatibility_mode, _phantom: PhantomData }
	}
}

#[async_trait::async_trait]
impl<B: BlockT, C, P, GetSlotFn> Verifier<B> for AuraVerifier<C, P, GetSlotFn, NumberFor<B>>
where
	C: ProvideRuntimeApi<B> + Send + Sync + sc_client_api::backend::AuxStore,
	C::Api: BlockBuilderApi<B> + AuraApi<B, AuthorityId<P>> + ApiExt<B>,
	P: Pair,
	P::Public: Codec + Debug,
	P::Signature: Codec,
	GetSlotFn: Fn(B::Hash) -> sp_blockchain::Result<Slot> + Send + Sync,
{
	async fn verify(
		&self,
		mut block: BlockImportParams<B>,
	) -> Result<BlockImportParams<B>, String> {
		// Skip checks that include execution, if being told so or when importing only state.
		//
		// This is done for example when gap syncing and it is expected that the block after the gap
		// was checked/chosen properly, e.g. by warp syncing to this block using a finality proof.
		// Or when we are importing state only and can not verify the seal.
		if block.with_state() || block.state_action.skip_execution_checks() {
			// When we are importing only the state of a block, it will be the best block.
			block.fork_choice = Some(ForkChoiceStrategy::Custom(block.with_state()));

			return Ok(block)
		}

		let hash = block.header.hash();
		let parent_hash = *block.header.parent_hash();
		let authorities = authorities(
			self.client.as_ref(),
			parent_hash,
			*block.header.number(),
			&self.compatibility_mode,
		)
		.map_err(|e| format!("Could not fetch authorities at {:?}: {}", parent_hash, e))?;

		let slot_now = (self.get_slot)(parent_hash)
			.map_err(|e| format!("Could not get slot for parent hash {:?}: {}", parent_hash, e))?;

		// we add one to allow for some small drift.
		// FIXME #1019 in the future, alter this queue to allow deferring of
		// headers
		let checked_header =
			check_header::<C, B, P>(slot_now + 1, block.header, hash, &authorities[..])
				.map_err(|e| e.to_string())?;
		match checked_header {
			CheckedHeader::Checked(pre_header, (_, seal)) => {
				trace!(target: LOG_TARGET, "Checked {:?}; importing.", pre_header);
				telemetry!(
					self.telemetry;
					CONSENSUS_TRACE;
					"aura.checked_and_importing";
					"pre_header" => ?pre_header,
				);

				block.header = pre_header;
				block.post_digests.push(seal);
				block.fork_choice = Some(ForkChoiceStrategy::LongestChain);
				block.post_hash = Some(hash);

				Ok(block)
			},
			CheckedHeader::Deferred(a, b) => {
				debug!(target: LOG_TARGET, "Checking {:?} failed; {:?}, {:?}.", hash, a, b);
				telemetry!(
					self.telemetry;
					CONSENSUS_DEBUG;
					"aura.header_too_far_in_future";
					"hash" => ?hash,
					"a" => ?a,
					"b" => ?b,
				);
				Err(format!("Header {:?} rejected: too far in the future", hash))
			},
		}
	}
}

/// Parameters of [`import_queue`].
pub struct ImportQueueParams<'a, Block: BlockT, I, C, S, GetSlotFn> {
	/// The block import to use.
	pub block_import: I,
	/// The justification import.
	pub justification_import: Option<BoxJustificationImport<Block>>,
	/// The client to interact with the chain.
	pub client: Arc<C>,
	/// Something that can get the current slot.
	pub get_slot: GetSlotFn,
	/// The spawner to spawn background tasks.
	pub spawner: &'a S,
	/// The prometheus registry.
	pub registry: Option<&'a Registry>,
	/// Telemetry instance used to report telemetry metrics.
	pub telemetry: Option<TelemetryHandle>,
	/// Compatibility mode that should be used.
	///
	/// If in doubt, use `Default::default()`.
	pub compatibility_mode: CompatibilityMode<NumberFor<Block>>,
}

/// Start an import queue for the Aura consensus algorithm.
pub fn import_queue<P, Block, I, C, S, GetSlotFn>(
	ImportQueueParams {
		block_import,
		justification_import,
		client,
		get_slot,
		spawner,
		registry,
		telemetry,
		compatibility_mode,
	}: ImportQueueParams<Block, I, C, S, GetSlotFn>,
) -> Result<DefaultImportQueue<Block>, sp_consensus::Error>
where
	Block: BlockT,
	C::Api: BlockBuilderApi<Block> + AuraApi<Block, AuthorityId<P>> + ApiExt<Block>,
	C: 'static
		+ ProvideRuntimeApi<Block>
		+ BlockOf
		+ Send
		+ Sync
		+ AuxStore
		+ UsageProvider<Block>
		+ HeaderBackend<Block>,
	I: BlockImport<Block, Error = ConsensusError> + Send + Sync + 'static,
	P: Pair + Send + Sync + 'static,
	P::Public: Codec + Debug,
	P::Signature: Codec,
	S: sp_core::traits::SpawnEssentialNamed,
	GetSlotFn: Fn(Block::Hash) -> sp_blockchain::Result<Slot> + Send + Sync + 'static,
{
	let verifier = build_verifier::<P, _, _, _>(BuildVerifierParams {
		client: client.clone(),
		get_slot,
		telemetry,
		compatibility_mode,
	});

	let cidp_client = client.clone();
	let create_inherent_data_providers = move |parent_hash, _| {
		let cidp_client = cidp_client.clone();
		async move {
			let slot_duration = crate::standalone::slot_duration_at(&*cidp_client, parent_hash)?;
			let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

			let slot =
						sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
							*timestamp,
							slot_duration,
						);

			Ok((slot, timestamp))
		}
	};
	let block_import =
		AuraBlockImport::<_, _, _, _, P>::new(block_import, client, create_inherent_data_providers);

	Ok(BasicQueue::new(verifier, Box::new(block_import), justification_import, spawner, registry))
}

/// Parameters of [`build_verifier`].
pub struct BuildVerifierParams<C, GetSlotFn, N> {
	/// The client to interact with the chain.
	pub client: Arc<C>,
	/// Something that can get the current slot.
	pub get_slot: GetSlotFn,
	/// Telemetry instance used to report telemetry metrics.
	pub telemetry: Option<TelemetryHandle>,
	/// Compatibility mode that should be used.
	///
	/// If in doubt, use `Default::default()`.
	pub compatibility_mode: CompatibilityMode<N>,
}

/// Build the [`AuraVerifier`]
pub fn build_verifier<P, C, GetSlotFn, N>(
	BuildVerifierParams { client, get_slot, telemetry, compatibility_mode }: BuildVerifierParams<
		C,
		GetSlotFn,
		N,
	>,
) -> AuraVerifier<C, P, GetSlotFn, N> {
	AuraVerifier::<_, P, _, _>::new(client, get_slot, telemetry, compatibility_mode)
}

struct AuraBlockImport<Block: BlockT, BI, Client, CIDP, P> {
	inner: BI,
	client: Arc<Client>,
	create_inherent_data_providers: CIDP,
	_phantom: PhantomData<(Block, P)>,
}

impl<Block: BlockT, BI, Client, CIDP, P> AuraBlockImport<Block, BI, Client, CIDP, P> {
	fn new(inner: BI, client: Arc<Client>, create_inherent_data_providers: CIDP) -> Self {
		Self { inner, client, create_inherent_data_providers, _phantom: Default::default() }
	}
}

#[async_trait::async_trait]
impl<Block: BlockT, BI, Client, CIDP, P> BlockImport<Block>
	for AuraBlockImport<Block, BI, Client, CIDP, P>
where
	Client: ProvideRuntimeApi<Block> + AuxStore + Send + Sync,
	<Client as ProvideRuntimeApi<Block>>::Api:
		BlockBuilderApi<Block> + AuraApi<Block, AuthorityId<P>>,
	BI: sc_consensus::BlockImport<Block, Error = sp_consensus::Error> + Send + Sync,
	CIDP: CreateInherentDataProviders<Block, ()> + Sync,
	CIDP::InherentDataProviders: InherentDataProviderExt,
	P: Pair + Send + Sync + 'static,
	P::Public: Codec + Debug,
	P::Signature: Codec,
{
	type Error = sp_consensus::Error;

	async fn check_block(
		&self,
		block: BlockCheckParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).await
	}

	async fn import_block(
		&self,
		mut block_params: BlockImportParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		let inherent_data_providers = self
			.create_inherent_data_providers
			.create_inherent_data_providers(*block_params.header.parent_hash(), ())
			.await
			.map_err(sp_consensus::Error::Other)?;

		// Check inherents.
		if let Some(inner_body) = block_params.body.take() {
			let inherent_data = inherent_data_providers
				.create_inherent_data()
				.await
				.map_err(|e| sp_consensus::Error::Other(e.into()))?;

			let block = Block::new(block_params.header.clone(), inner_body);

			let inherent_res = self
				.client
				.runtime_api()
				.check_inherents(*block.header().parent_hash(), block.clone(), inherent_data)
				.map_err(|e| sp_consensus::Error::Other(Box::new(e)))?;

			if !inherent_res.ok() {
				for (i, e) in inherent_res.into_errors() {
					match inherent_data_providers.try_handle_error(&i, &e).await {
						Some(res) => res.map_err(|e| sp_consensus::Error::InvalidInherents(e))?,
						None => return Err(sp_consensus::Error::InvalidInherentsUnhandled(i)),
					}
				}
			}

			let (_, inner_body) = block.deconstruct();
			block_params.body = Some(inner_body);
		}

		// Check equivocations.
		let slot_now = inherent_data_providers.slot();
		let authorities = authorities(
			self.client.as_ref(),
			*block_params.header.parent_hash(),
			*block_params.header.number(),
			&Default::default(),
		)?;
		// we add one to allow for some small drift.
		// FIXME #1019 in the future, alter this queue to allow deferring of
		// headers
		let hash = block_params.header.hash();
		block_params.header = check_header_equivocation::<Client, Block, P>(
			&self.client,
			slot_now + 1,
			block_params.header,
			hash,
			&authorities[..],
		)
		.map_err(|e| Self::Error::Other(e.into()))?;

		self.inner.import_block(block_params).await
	}
}

/// Check that the header is valid and not an equivocation.
fn check_header_equivocation<C, B: BlockT, P: Pair>(
	client: &C,
	slot_now: Slot,
	header: B::Header,
	hash: B::Hash,
	authorities: &[AuthorityId<P>],
) -> Result<B::Header, Error<B>>
where
	P::Public: Codec,
	P::Signature: Codec,
	C: sc_client_api::backend::AuxStore,
{
	let check_result =
		crate::standalone::check_header_slot_and_seal::<B, P>(slot_now, header, authorities);

	match check_result {
		Ok((header, slot, _)) => {
			let expected_author = crate::standalone::slot_author::<P>(slot, &authorities);
			if let Some(expected) = expected_author {
				if let Some(equivocation_proof) =
					check_equivocation(client, slot_now, slot, &header, expected)
						.map_err(Error::Client)?
				{
					log::info!(
						target: LOG_TARGET,
						"Slot author is equivocating at slot {} with headers {:?} and {:?}",
						slot,
						equivocation_proof.first_header.hash(),
						equivocation_proof.second_header.hash(),
					);
				}
			}
			Ok(header)
		},
		Err(SealVerificationError::Deferred(header, _)) => Ok(header),
		Err(SealVerificationError::Unsealed) => Err(Error::HeaderUnsealed(hash)),
		Err(SealVerificationError::BadSeal) => Err(Error::HeaderBadSeal(hash)),
		Err(SealVerificationError::BadSignature) => Err(Error::BadSignature(hash)),
		Err(SealVerificationError::SlotAuthorNotFound) => Err(Error::SlotAuthorNotFound),
		Err(SealVerificationError::InvalidPreDigest(e)) => Err(Error::from(e)),
	}
}
