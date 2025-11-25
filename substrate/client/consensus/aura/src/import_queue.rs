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
	standalone::SealVerificationError, AuthoritiesTracker, AuthorityId, CompatibilityMode, Error,
	LOG_TARGET,
};
use codec::Codec;
use log::{debug, info, trace};
use prometheus_endpoint::Registry;
use sc_client_api::{backend::AuxStore, BlockOf, UsageProvider};
use sc_consensus::{
	block_import::{BlockImport, BlockImportParams, ForkChoiceStrategy},
	import_queue::{BasicQueue, BoxJustificationImport, DefaultImportQueue, Verifier},
	BlockCheckParams, ImportResult, StateAction,
};
use sc_consensus_slots::{check_equivocation, CheckedHeader, InherentDataProviderExt};
use sc_telemetry::{telemetry, TelemetryHandle, CONSENSUS_DEBUG, CONSENSUS_TRACE};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::{HeaderBackend, HeaderMetadata};
use sp_consensus::Error as ConsensusError;
use sp_consensus_aura::{inherents::AuraInherentData, AuraApi};
use sp_consensus_slots::Slot;
use sp_core::crypto::Pair;
use sp_inherents::{CreateInherentDataProviders, InherentDataProvider as _};
use sp_runtime::{
	traits::{Block as BlockT, Header, NumberFor},
	DigestItem,
};
use std::{fmt::Debug, sync::Arc};

/// check a header has been signed by the right key. If the slot is too far in the future, an error
/// will be returned. If it's successful, returns the pre-header and the digest item
/// containing the seal.
///
/// This digest item will always return `Some` when used with `as_aura_seal`.
fn check_header<C, B: BlockT, P: Pair>(
	client: &C,
	slot_now: Slot,
	header: B::Header,
	hash: B::Hash,
	authorities: &[AuthorityId<P>],
	check_for_equivocation: CheckForEquivocation,
) -> Result<CheckedHeader<B::Header, (Slot, DigestItem)>, Error<B>>
where
	P::Public: Codec,
	P::Signature: Codec,
	C: sc_client_api::backend::AuxStore,
{
	let check_result =
		crate::standalone::check_header_slot_and_seal::<B, P>(slot_now, header, authorities);

	match check_result {
		Ok((header, slot, seal)) => {
			let expected_author = crate::standalone::slot_author::<P>(slot, &authorities);
			let should_equiv_check = check_for_equivocation.check_for_equivocation();
			if let (true, Some(expected)) = (should_equiv_check, expected_author) {
				if let Some(equivocation_proof) =
					check_equivocation(client, slot_now, slot, &header, expected)
						.map_err(Error::Client)?
				{
					info!(
						target: LOG_TARGET,
						"Slot author is equivocating at slot {} with headers {:?} and {:?}",
						slot,
						equivocation_proof.first_header.hash(),
						equivocation_proof.second_header.hash(),
					);
				}
			}

			Ok(CheckedHeader::Checked(header, (slot, seal)))
		},
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
pub struct AuraVerifier<C, P: Pair, CIDP, B: BlockT> {
	client: Arc<C>,
	create_inherent_data_providers: CIDP,
	check_for_equivocation: CheckForEquivocation,
	telemetry: Option<TelemetryHandle>,
	authorities_tracker: Arc<AuthoritiesTracker<P, B, C>>,
}

impl<C, P: Pair, CIDP, B: BlockT> AuraVerifier<C, P, CIDP, B>
where
	C: HeaderBackend<B> + HeaderMetadata<B, Error = sp_blockchain::Error> + ProvideRuntimeApi<B>,
	P::Public: Codec + Debug,
	C::Api: AuraApi<B, AuthorityId<P>>,
{
	/// Create a new Aura verifier.
	pub fn new(
		client: Arc<C>,
		create_inherent_data_providers: CIDP,
		check_for_equivocation: CheckForEquivocation,
		telemetry: Option<TelemetryHandle>,
		authorities_tracker: Arc<AuthoritiesTracker<P, B, C>>,
	) -> Result<Self, String> {
		Ok(Self {
			client: client.clone(),
			create_inherent_data_providers,
			check_for_equivocation,
			telemetry,
			authorities_tracker,
		})
	}
}

#[async_trait::async_trait]
impl<B, C, P, CIDP> Verifier<B> for AuraVerifier<C, P, CIDP, B>
where
	B: BlockT,
	C: HeaderBackend<B>
		+ HeaderMetadata<B, Error = sp_blockchain::Error>
		+ ProvideRuntimeApi<B>
		+ Send
		+ Sync
		+ sc_client_api::backend::AuxStore,
	C::Api: BlockBuilderApi<B> + AuraApi<B, AuthorityId<P>> + ApiExt<B>,
	P: Pair,
	P::Public: Codec + Debug,
	P::Signature: Codec,
	CIDP: CreateInherentDataProviders<B, ()> + Send + Sync,
	CIDP::InherentDataProviders: InherentDataProviderExt + Send + Sync,
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
		let number = *block.header.number();

		let authorities = self.authorities_tracker.fetch(&block.header).map_err(|e| {
			format!("Could not fetch authorities for block {hash:?} at number {number}: {e}")
		})?;

		let create_inherent_data_providers = self
			.create_inherent_data_providers
			.create_inherent_data_providers(parent_hash, ())
			.await
			.map_err(|e| Error::<B>::Client(sp_blockchain::Error::Application(e)))?;

		let mut inherent_data = create_inherent_data_providers
			.create_inherent_data()
			.await
			.map_err(Error::<B>::Inherent)?;

		let slot_now = create_inherent_data_providers.slot();

		// we add one to allow for some small drift.
		// FIXME #1019 in the future, alter this queue to allow deferring of
		// headers
		let checked_header = check_header::<C, B, P>(
			&self.client,
			slot_now + 1,
			block.header,
			hash,
			&authorities[..],
			self.check_for_equivocation,
		)
		.map_err(|e| e.to_string())?;
		match checked_header {
			CheckedHeader::Checked(pre_header, (slot, seal)) => {
				// if the body is passed through, we need to use the runtime
				// to check that the internally-set timestamp in the inherents
				// actually matches the slot set in the seal.
				if let Some(inner_body) = block.body.take() {
					let new_block = B::new(pre_header.clone(), inner_body);

					inherent_data.aura_replace_inherent_data(slot);

					// skip the inherents verification if the runtime API is old or not expected to
					// exist.
					if self
						.client
						.runtime_api()
						.has_api_with::<dyn BlockBuilderApi<B>, _>(parent_hash, |v| v >= 2)
						.map_err(|e| e.to_string())?
					{
						sp_block_builder::check_inherents_with_data(
							self.client.clone(),
							parent_hash,
							new_block.clone(),
							&create_inherent_data_providers,
							inherent_data,
						)
						.await
						.map_err(|e| format!("Error checking block inherents {:?}", e))?;
					}

					let (_, inner_body) = new_block.deconstruct();
					block.body = Some(inner_body);
				}

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

/// Should we check for equivocation of a block author?
#[derive(Debug, Clone, Copy)]
pub enum CheckForEquivocation {
	/// Yes, check for equivocation.
	///
	/// This is the default setting for this.
	Yes,
	/// No, don't check for equivocation.
	No,
}

impl CheckForEquivocation {
	/// Should we check for equivocation?
	fn check_for_equivocation(self) -> bool {
		matches!(self, Self::Yes)
	}
}

impl Default for CheckForEquivocation {
	fn default() -> Self {
		Self::Yes
	}
}

/// Parameters of [`import_queue`].
pub struct ImportQueueParams<'a, Block: BlockT, I, C, S, CIDP> {
	/// The block import to use.
	pub block_import: I,
	/// The justification import.
	pub justification_import: Option<BoxJustificationImport<Block>>,
	/// The client to interact with the chain.
	pub client: Arc<C>,
	/// Something that can create the inherent data providers.
	pub create_inherent_data_providers: CIDP,
	/// The spawner to spawn background tasks.
	pub spawner: &'a S,
	/// The prometheus registry.
	pub registry: Option<&'a Registry>,
	/// Should we check for equivocation?
	pub check_for_equivocation: CheckForEquivocation,
	/// Telemetry instance used to report telemetry metrics.
	pub telemetry: Option<TelemetryHandle>,
	/// Compatibility mode that should be used.
	///
	/// If in doubt, use `Default::default()`.
	pub compatibility_mode: CompatibilityMode<NumberFor<Block>>,
}

/// Start an import queue for the Aura consensus algorithm.
pub fn import_queue<P, Block, I, C, S, CIDP>(
	ImportQueueParams {
		block_import,
		justification_import,
		client,
		create_inherent_data_providers,
		spawner,
		registry,
		check_for_equivocation,
		telemetry,
		compatibility_mode,
	}: ImportQueueParams<Block, I, C, S, CIDP>,
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
		+ HeaderBackend<Block>
		+ HeaderMetadata<Block, Error = sp_blockchain::Error>,
	I: BlockImport<Block, Error = ConsensusError> + Send + Sync + 'static + Clone,
	P: Pair + 'static + Clone,
	P::Public: Codec + Debug,
	P::Signature: Codec,
	S: sp_core::traits::SpawnEssentialNamed,
	CIDP: CreateInherentDataProviders<Block, ()> + Sync + Send + 'static,
	CIDP::InherentDataProviders: InherentDataProviderExt + Send + Sync,
{
	let verifier = build_verifier::<P, _, _, _>(BuildVerifierParams {
		client,
		create_inherent_data_providers,
		check_for_equivocation,
		telemetry,
		compatibility_mode,
	})
	.map_err(|e| sp_consensus::Error::Other(e.into()))?;

	let authorities_tracker = verifier.authorities_tracker.clone();

	let block_import = AuraBlockImport { block_import, authorities_tracker };

	Ok(BasicQueue::new(verifier, Box::new(block_import), justification_import, spawner, registry))
}

/// AURA block import.
pub struct AuraBlockImport<Client, P: Pair + Clone, Block: BlockT, BI: BlockImport<Block> + Clone> {
	block_import: BI,
	authorities_tracker: Arc<AuthoritiesTracker<P, Block, Client>>,
}

impl<Client, P: Pair + Clone, Block: BlockT, BI: BlockImport<Block> + Clone> Clone
	for AuraBlockImport<Client, P, Block, BI>
{
	fn clone(&self) -> Self {
		Self {
			block_import: self.block_import.clone(),
			authorities_tracker: self.authorities_tracker.clone(),
		}
	}
}

impl<
		Client: Sync + Send,
		P: Pair + Clone,
		Block: BlockT,
		BI: BlockImport<Block> + Send + Sync + Clone,
	> AuraBlockImport<Client, P, Block, BI>
where
	Client: HeaderBackend<Block>
		+ HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ ProvideRuntimeApi<Block>,
	P::Public: Codec + Debug,
	P::Signature: Codec,
	Client::Api: AuraApi<Block, AuthorityId<P>>,
{
	/// Create a new AURA block import.
	pub fn new(
		block_import: BI,
		authorities_tracker: Arc<AuthoritiesTracker<P, Block, Client>>,
	) -> Self {
		Self { block_import, authorities_tracker }
	}
}

#[async_trait::async_trait]
impl<
		Client: Sync + Send,
		P: Pair + Clone,
		Block: BlockT,
		BI: BlockImport<Block> + Send + Sync + Clone,
	> BlockImport<Block> for AuraBlockImport<Client, P, Block, BI>
where
	Client: HeaderBackend<Block>
		+ HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ ProvideRuntimeApi<Block>,
	P::Public: Codec + Debug,
	P::Signature: Codec,
	Client::Api: AuraApi<Block, AuthorityId<P>>,
	BI::Error: From<String>,
{
	type Error = BI::Error;

	async fn check_block(
		&self,
		block: BlockCheckParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		self.block_import.check_block(block).await
	}

	/// Import a block.
	async fn import_block(
		&self,
		block: BlockImportParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		let post_header = block.post_header();
		let import_from_runtime = matches!(block.state_action, StateAction::ApplyChanges(_));

		log::info!("Importing block with state {with_state}");
		let res = self.block_import.import_block(block).await?;

		if import_from_runtime {
			self.authorities_tracker.import_from_runtime(&post_header)?;
		}

		Ok(res)
	}
}

/// Parameters of [`build_verifier`].
pub struct BuildVerifierParams<C, CIDP, N> {
	/// The client to interact with the chain.
	pub client: Arc<C>,
	/// Something that can create the inherent data providers.
	pub create_inherent_data_providers: CIDP,
	/// Should we check for equivocation?
	pub check_for_equivocation: CheckForEquivocation,
	/// Telemetry instance used to report telemetry metrics.
	pub telemetry: Option<TelemetryHandle>,
	/// Compatibility mode that should be used.
	///
	/// If in doubt, use `Default::default()`.
	pub compatibility_mode: CompatibilityMode<N>,
}

/// Build the [`AuraVerifier`]
pub fn build_verifier<P: Pair, C, CIDP, B: BlockT>(
	BuildVerifierParams {
		client,
		create_inherent_data_providers,
		check_for_equivocation,
		telemetry,
		compatibility_mode,
	}: BuildVerifierParams<C, CIDP, NumberFor<B>>,
) -> Result<AuraVerifier<C, P, CIDP, B>, String>
where
	C: HeaderBackend<B> + HeaderMetadata<B, Error = sp_blockchain::Error> + ProvideRuntimeApi<B>,
	P::Public: Codec + Debug,
	C::Api: AuraApi<B, AuthorityId<P>>,
{
	AuraVerifier::<_, P, _, _>::new(
		client.clone(),
		create_inherent_data_providers,
		check_for_equivocation,
		telemetry,
		Arc::new(AuthoritiesTracker::new(client, &compatibility_mode)?),
	)
}
