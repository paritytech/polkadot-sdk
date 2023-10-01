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

//! Module implementing the logic for verifying and importing AURA blocks.

use crate::{
	authorities, standalone::SealVerificationError, AuthorityId, CompatibilityMode, Error,
	LOG_TARGET,
};
use codec::Codec;
use log::{debug, info, trace, warn};
use prometheus_endpoint::Registry;
use sc_client_api::{backend::AuxStore, BlockOf, UsageProvider};
use sc_consensus::{
	block_import::{BlockImport, BlockImportParams, ForkChoiceStrategy},
	import_queue::{BasicQueue, BoxJustificationImport, DefaultImportQueue, Verifier},
};
use sc_consensus_slots::{check_equivocation, CheckedHeader, InherentDataProviderExt};
use sc_telemetry::{telemetry, TelemetryHandle, CONSENSUS_DEBUG, CONSENSUS_TRACE};
use sc_transaction_pool_api::OffchainTransactionPoolFactory;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::HeaderBackend;
use sp_consensus::{BlockOrigin, Error as ConsensusError, SelectChain};
use sp_consensus_aura::{inherents::AuraInherentData, AuraApi};
use sp_consensus_slots::Slot;
use sp_core::crypto::Pair;
use sp_inherents::{CreateInherentDataProviders, InherentDataProvider as _};
use sp_runtime::{
	traits::{Block as BlockT, Header, NumberFor},
	DigestItem,
};
use std::{fmt::Debug, marker::PhantomData, sync::Arc};

// Checked header return information.
struct VerifiedHeaderInfo<P: Pair> {
	slot: Slot,
	seal: DigestItem,
	author: AuthorityId<P>,
}

/// Check if a header has been signed by the right key.
///
/// If the slot is too far in the future, an error will be returned.
/// If it's successful, returns the checked header and some information
/// which is required by the current callers.
fn check_header<B: BlockT, P: Pair>(
	slot_now: Slot,
	header: B::Header,
	hash: B::Hash,
	authorities: &[AuthorityId<P>],
) -> Result<CheckedHeader<B::Header, VerifiedHeaderInfo<P>>, Error<B>>
where
	P::Public: Codec,
	P::Signature: Codec,
{
	let check_result =
		crate::standalone::check_header_slot_and_seal::<B, P>(slot_now, header, authorities);

	match check_result {
		Ok((header, slot, seal)) => {
			let author = crate::standalone::slot_author::<P>(slot, &authorities)
				.ok_or(Error::SlotAuthorNotFound)?
				.clone();
			Ok(CheckedHeader::Checked(header, VerifiedHeaderInfo { slot, seal, author }))
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

/// A verifier for AURA blocks.
pub struct AuraVerifier<B: BlockT, C, P, SC, CIDP, N> {
	client: Arc<C>,
	select_chain: SC,
	create_inherent_data_providers: CIDP,
	check_for_equivocation: CheckForEquivocation,
	telemetry: Option<TelemetryHandle>,
	offchain_tx_pool_factory: OffchainTransactionPoolFactory<B>,
	compatibility_mode: CompatibilityMode<N>,
	_phantom: PhantomData<fn() -> P>,
}

impl<B: BlockT, C, P, SC, CIDP, N> AuraVerifier<B, C, P, SC, CIDP, N> {
	pub(crate) fn new(
		client: Arc<C>,
		select_chain: SC,
		create_inherent_data_providers: CIDP,
		check_for_equivocation: CheckForEquivocation,
		telemetry: Option<TelemetryHandle>,
		offchain_tx_pool_factory: OffchainTransactionPoolFactory<B>,
		compatibility_mode: CompatibilityMode<N>,
	) -> Self {
		Self {
			client,
			select_chain,
			create_inherent_data_providers,
			check_for_equivocation,
			telemetry,
			offchain_tx_pool_factory,
			compatibility_mode,
			_phantom: PhantomData,
		}
	}
}

impl<B, C, P, SC, CIDP, N> AuraVerifier<B, C, P, SC, CIDP, N>
where
	B: BlockT,
	C: ProvideRuntimeApi<B> + AuxStore,
	C::Api: AuraApi<B, AuthorityId<P>> + BlockBuilderApi<B>,
	P: Pair,
	P::Public: Codec,
	SC: SelectChain<B>,
	CIDP: CreateInherentDataProviders<B, ()>,
	CIDP: Send,
{
	async fn check_inherents(
		&self,
		block: B,
		at_hash: B::Hash,
		inherent_data: sp_inherents::InherentData,
		create_inherent_data_providers: CIDP::InherentDataProviders,
	) -> Result<(), Error<B>> {
		let inherent_res = self
			.client
			.runtime_api()
			.check_inherents(at_hash, block, inherent_data)
			.map_err(|e| Error::Client(e.into()))?;

		if !inherent_res.ok() {
			for (i, e) in inherent_res.into_errors() {
				match create_inherent_data_providers.try_handle_error(&i, &e).await {
					Some(res) => res.map_err(Error::Inherent)?,
					None => return Err(Error::UnknownInherentError(i)),
				}
			}
		}

		Ok(())
	}

	async fn check_and_report_equivocation(
		&self,
		slot_now: Slot,
		slot: Slot,
		header: &B::Header,
		author: &AuthorityId<P>,
		origin: &BlockOrigin,
	) -> Result<(), Error<B>> {
		// Don't report any equivocations during initial sync as they are most likely stale.
		if !self.check_for_equivocation.0 || *origin == BlockOrigin::NetworkInitialSync {
			return Ok(())
		}

		// Check if authorship of this header is an equivocation and return a proof if so.
		let Some(equivocation_proof) =
			check_equivocation(&*self.client, slot_now, slot, header, author)
				.map_err(Error::Client)?
		else {
			return Ok(())
		};

		info!(
			target: LOG_TARGET,
			"Equivocation at slot {} with headers {:?} and {:?}",
			slot,
			equivocation_proof.first_header.hash(),
			equivocation_proof.second_header.hash(),
		);

		// Get the best block on which we will build and send the equivocation report.
		let best_hash = self
			.select_chain
			.best_chain()
			.await
			.map(|h| h.hash())
			.map_err(|e| Error::Client(e.into()))?;

		let mut runtime_api = self.client.runtime_api();

		// Generate a key ownership proof. We start by trying to generate the
		// key ownership proof at the parent of the equivocating header, this
		// will make sure that proof generation is successful since it happens
		// during the on-going session (i.e. session keys are available in the
		// state to be able to generate the proof). This might fail if the
		// equivocation happens on the first block of the session, in which case
		// its parent would be on the previous session. If generation on the
		// parent header fails we try with best block as well.
		let generate_key_owner_proof = |at_hash| {
			runtime_api
				.generate_key_ownership_proof(at_hash, slot, equivocation_proof.offender.clone())
				.map_err(Error::<B>::RuntimeApi)
		};

		let parent_hash = *header.parent_hash();
		let key_owner_proof = match generate_key_owner_proof(parent_hash)? {
			Some(proof) => proof,
			None => match generate_key_owner_proof(best_hash)? {
				Some(proof) => proof,
				None => {
					warn!(
						target: LOG_TARGET,
						"Equivocation offender is not part of the authority set."
					);
					return Ok(())
				},
			},
		};

		// Register the offchain tx pool to be able to use it from the runtime.
		runtime_api
			.register_extension(self.offchain_tx_pool_factory.offchain_transaction_pool(best_hash));

		// Submit equivocation report at best block.
		runtime_api
			.submit_report_equivocation_unsigned_extrinsic(
				best_hash,
				equivocation_proof,
				key_owner_proof,
			)
			.map_err(Error::RuntimeApi)?;

		Ok(())
	}
}

#[async_trait::async_trait]
impl<B: BlockT, C, P, SC, CIDP> Verifier<B> for AuraVerifier<B, C, P, SC, CIDP, NumberFor<B>>
where
	C: ProvideRuntimeApi<B> + Send + Sync + AuxStore,
	C::Api: BlockBuilderApi<B> + AuraApi<B, AuthorityId<P>> + ApiExt<B>,
	SC: SelectChain<B>,
	P: Pair,
	P::Public: Codec + Debug,
	P::Signature: Codec,
	CIDP: CreateInherentDataProviders<B, ()> + Send + Sync,
	CIDP::InherentDataProviders: InherentDataProviderExt + Send + Sync,
{
	async fn verify(
		&mut self,
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

		let create_inherent_data_providers = self
			.create_inherent_data_providers
			.create_inherent_data_providers(parent_hash, ())
			.await
			.map_err(|e| Error::<B>::Client(sp_blockchain::Error::Application(e)))?;

		let slot_now = create_inherent_data_providers.slot();

		// We add one to allow for some small drift.
		// FIXME #1019 in the future, alter this queue to allow deferring of headers
		let checked_header =
			check_header::<B, P>(slot_now + 1, block.header.clone(), hash, &authorities)
				.map_err(|e| e.to_string())?;

		match checked_header {
			CheckedHeader::Checked(pre_header, verified_info) => {
				if let Err(err) = self
					.check_and_report_equivocation(
						slot_now,
						verified_info.slot,
						&block.header,
						&verified_info.author,
						&block.origin,
					)
					.await
				{
					warn!(target: LOG_TARGET, "Error checking/reporting AURA equivocation: {}", err)
				};

				// If the body is passed through, we need to use the runtime
				// to check that the internally-set timestamp in the inherents
				// actually matches the slot set in the seal.
				if let Some(inner_body) = block.body {
					let new_block = B::new(pre_header.clone(), inner_body);

					// Skip the inherents verification if the runtime API is old or not expected to
					// exist.
					if self
						.client
						.runtime_api()
						.has_api_with::<dyn BlockBuilderApi<B>, _>(parent_hash, |v| v >= 2)
						.map_err(|e| e.to_string())?
					{
						let mut inherent_data = create_inherent_data_providers
							.create_inherent_data()
							.await
							.map_err(Error::<B>::Inherent)?;

						inherent_data.aura_replace_inherent_data(verified_info.slot);

						self.check_inherents(
							new_block.clone(),
							parent_hash,
							inherent_data,
							create_inherent_data_providers,
						)
						.await
						.map_err(|e| e.to_string())?;
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
				block.post_digests.push(verified_info.seal);
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
///
/// Implemented as a `bool` newtype (default: true)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckForEquivocation(bool);

impl Default for CheckForEquivocation {
	fn default() -> Self {
		Self(true)
	}
}

impl From<bool> for CheckForEquivocation {
	fn from(value: bool) -> Self {
		Self(value)
	}
}

impl From<CheckForEquivocation> for bool {
	fn from(value: CheckForEquivocation) -> Self {
		value.0
	}
}

/// Parameters of [`import_queue`].
pub struct ImportQueueParams<'a, B: BlockT, I, C, SC, S, CIDP> {
	/// The block import to use.
	pub block_import: I,
	/// The justification import.
	pub justification_import: Option<BoxJustificationImport<B>>,
	/// The client to interact with the chain.
	pub client: Arc<C>,
	/// Chain selection system.
	pub select_chain: SC,
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
	/// The offchain transaction pool factory.
	///
	/// Will be used when sending equivocation reports.
	pub offchain_tx_pool_factory: OffchainTransactionPoolFactory<B>,
	/// Compatibility mode that should be used.
	///
	/// If in doubt, use `Default::default()`.
	pub compatibility_mode: CompatibilityMode<NumberFor<B>>,
}

/// Start an import queue for the AURA consensus algorithm.
pub fn import_queue<P, B, I, C, SC, S, CIDP>(
	ImportQueueParams {
		block_import,
		justification_import,
		client,
		select_chain,
		create_inherent_data_providers,
		spawner,
		registry,
		check_for_equivocation,
		telemetry,
		offchain_tx_pool_factory,
		compatibility_mode,
	}: ImportQueueParams<B, I, C, SC, S, CIDP>,
) -> Result<DefaultImportQueue<B>, sp_consensus::Error>
where
	B: BlockT,
	C::Api: BlockBuilderApi<B> + AuraApi<B, AuthorityId<P>> + ApiExt<B>,
	C: 'static
		+ ProvideRuntimeApi<B>
		+ BlockOf
		+ Send
		+ Sync
		+ AuxStore
		+ UsageProvider<B>
		+ HeaderBackend<B>,
	SC: SelectChain<B> + 'static,
	I: BlockImport<B, Error = ConsensusError> + Send + Sync + 'static,
	P: Pair + 'static,
	P::Public: Codec + Debug,
	P::Signature: Codec,
	S: sp_core::traits::SpawnEssentialNamed,
	CIDP: CreateInherentDataProviders<B, ()> + Sync + Send + 'static,
	CIDP::InherentDataProviders: InherentDataProviderExt + Send + Sync,
{
	let verifier = build_verifier::<B, P, _, _, _, _>(BuildVerifierParams {
		client,
		select_chain,
		create_inherent_data_providers,
		check_for_equivocation,
		telemetry,
		offchain_tx_pool_factory,
		compatibility_mode,
	});

	Ok(BasicQueue::new(verifier, Box::new(block_import), justification_import, spawner, registry))
}

/// Parameters of [`build_verifier`].
pub struct BuildVerifierParams<B: BlockT, C, SC, CIDP, N> {
	/// The client to interact with the chain.
	pub client: Arc<C>,
	/// Chain selection system.
	pub select_chain: SC,
	/// Something that can create the inherent data providers.
	pub create_inherent_data_providers: CIDP,
	/// Should we check for equivocation?
	pub check_for_equivocation: CheckForEquivocation,
	/// Telemetry instance used to report telemetry metrics.
	pub telemetry: Option<TelemetryHandle>,
	/// The offchain transaction pool factory.
	///
	/// Will be used when sending equivocation reports.
	pub offchain_tx_pool_factory: OffchainTransactionPoolFactory<B>,
	/// Compatibility mode that should be used.
	///
	/// If in doubt, use `Default::default()`.
	pub compatibility_mode: CompatibilityMode<N>,
}

/// Build the [`AuraVerifier`]
pub fn build_verifier<B: BlockT, P, C, SC, CIDP, N>(
	BuildVerifierParams {
		client,
		select_chain,
		create_inherent_data_providers,
		check_for_equivocation,
		telemetry,
		offchain_tx_pool_factory,
		compatibility_mode,
	}: BuildVerifierParams<B, C, SC, CIDP, N>,
) -> AuraVerifier<B, C, P, SC, CIDP, N> {
	AuraVerifier::<B, _, P, _, _, _>::new(
		client,
		select_chain,
		create_inherent_data_providers,
		check_for_equivocation,
		telemetry,
		offchain_tx_pool_factory,
		compatibility_mode,
	)
}
