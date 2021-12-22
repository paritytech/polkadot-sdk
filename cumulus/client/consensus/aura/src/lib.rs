// Copyright 2021 Parity Technologies (UK) Ltd.
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

//! The AuRa consensus algoritm for parachains.
//!
//! This extends the Substrate provided AuRa consensus implementation to make it compatible for
//! parachains. The main entry points for of this consensus algorithm are [`build_aura_consensus`]
//! and [`import_queue`].
//!
//! For more information about AuRa, the Substrate crate should be checked.

use codec::{Decode, Encode};
use cumulus_client_consensus_common::{
	ParachainBlockImport, ParachainCandidate, ParachainConsensus,
};
use cumulus_primitives_core::{relay_chain::v1::Hash as PHash, PersistedValidationData};

use futures::lock::Mutex;
use sc_client_api::{backend::AuxStore, BlockOf};
use sc_consensus::BlockImport;
use sc_consensus_slots::{BackoffAuthoringBlocksStrategy, SlotInfo};
use sc_telemetry::TelemetryHandle;
use sp_api::ProvideRuntimeApi;
use sp_application_crypto::AppPublic;
use sp_blockchain::HeaderBackend;
use sp_consensus::{
	EnableProofRecording, Environment, ProofRecording, Proposer, SlotData, SyncOracle,
};
use sp_consensus_aura::AuraApi;
use sp_core::crypto::Pair;
use sp_inherents::{CreateInherentDataProviders, InherentData, InherentDataProvider};
use sp_keystore::SyncCryptoStorePtr;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, Member, NumberFor};
use std::{convert::TryFrom, hash::Hash, sync::Arc};

mod import_queue;

pub use import_queue::{build_verifier, import_queue, BuildVerifierParams, ImportQueueParams};
pub use sc_consensus_aura::{
	slot_duration, AuraVerifier, BuildAuraWorkerParams, SlotDuration, SlotProportion,
};
pub use sc_consensus_slots::InherentDataProviderExt;

const LOG_TARGET: &str = "aura::cumulus";

/// The implementation of the AURA consensus for parachains.
pub struct AuraConsensus<B, CIDP> {
	create_inherent_data_providers: Arc<CIDP>,
	aura_worker: Arc<
		Mutex<
			dyn sc_consensus_slots::SlotWorker<B, <EnableProofRecording as ProofRecording>::Proof>
				+ Send
				+ 'static,
		>,
	>,
	slot_duration: SlotDuration,
}

impl<B, CIDP> Clone for AuraConsensus<B, CIDP> {
	fn clone(&self) -> Self {
		Self {
			create_inherent_data_providers: self.create_inherent_data_providers.clone(),
			aura_worker: self.aura_worker.clone(),
			slot_duration: self.slot_duration,
		}
	}
}

impl<B, CIDP> AuraConsensus<B, CIDP>
where
	B: BlockT,
	CIDP: CreateInherentDataProviders<B, (PHash, PersistedValidationData)> + 'static,
	CIDP::InherentDataProviders: InherentDataProviderExt,
{
	/// Create a new boxed instance of AURA consensus.
	pub fn build<P, Client, BI, SO, PF, BS, Error>(
		BuildAuraConsensusParams {
			proposer_factory,
			create_inherent_data_providers,
			block_import,
			para_client,
			backoff_authoring_blocks,
			sync_oracle,
			keystore,
			force_authoring,
			slot_duration,
			telemetry,
			block_proposal_slot_portion,
			max_block_proposal_slot_portion,
		}: BuildAuraConsensusParams<PF, BI, CIDP, Client, BS, SO>,
	) -> Box<dyn ParachainConsensus<B>>
	where
		Client:
			ProvideRuntimeApi<B> + BlockOf + AuxStore + HeaderBackend<B> + Send + Sync + 'static,
		Client::Api: AuraApi<B, P::Public>,
		BI: BlockImport<B, Transaction = sp_api::TransactionFor<Client, B>> + Send + Sync + 'static,
		SO: SyncOracle + Send + Sync + Clone + 'static,
		BS: BackoffAuthoringBlocksStrategy<NumberFor<B>> + Send + Sync + 'static,
		PF: Environment<B, Error = Error> + Send + Sync + 'static,
		PF::Proposer: Proposer<
			B,
			Error = Error,
			Transaction = sp_api::TransactionFor<Client, B>,
			ProofRecording = EnableProofRecording,
			Proof = <EnableProofRecording as ProofRecording>::Proof,
		>,
		Error: std::error::Error + Send + From<sp_consensus::Error> + 'static,
		P: Pair + Send + Sync,
		P::Public: AppPublic + Hash + Member + Encode + Decode,
		P::Signature: TryFrom<Vec<u8>> + Hash + Member + Encode + Decode,
	{
		let worker = sc_consensus_aura::build_aura_worker::<P, _, _, _, _, _, _, _, _>(
			BuildAuraWorkerParams {
				client: para_client,
				block_import: ParachainBlockImport::new(block_import),
				justification_sync_link: (),
				proposer_factory,
				sync_oracle,
				force_authoring,
				backoff_authoring_blocks,
				keystore,
				telemetry,
				block_proposal_slot_portion,
				max_block_proposal_slot_portion,
			},
		);

		Box::new(Self {
			create_inherent_data_providers: Arc::new(create_inherent_data_providers),
			aura_worker: Arc::new(Mutex::new(worker)),
			slot_duration,
		})
	}

	/// Create the inherent data.
	///
	/// Returns the created inherent data and the inherent data providers used.
	async fn inherent_data(
		&self,
		parent: B::Hash,
		validation_data: &PersistedValidationData,
		relay_parent: PHash,
	) -> Option<(InherentData, CIDP::InherentDataProviders)> {
		let inherent_data_providers = self
			.create_inherent_data_providers
			.create_inherent_data_providers(parent, (relay_parent, validation_data.clone()))
			.await
			.map_err(|e| {
				tracing::error!(
					target: LOG_TARGET,
					error = ?e,
					"Failed to create inherent data providers.",
				)
			})
			.ok()?;

		inherent_data_providers
			.create_inherent_data()
			.map_err(|e| {
				tracing::error!(
					target: LOG_TARGET,
					error = ?e,
					"Failed to create inherent data.",
				)
			})
			.ok()
			.map(|d| (d, inherent_data_providers))
	}
}

#[async_trait::async_trait]
impl<B, CIDP> ParachainConsensus<B> for AuraConsensus<B, CIDP>
where
	B: BlockT,
	CIDP: CreateInherentDataProviders<B, (PHash, PersistedValidationData)> + Send + Sync + 'static,
	CIDP::InherentDataProviders: InherentDataProviderExt + Send,
{
	async fn produce_candidate(
		&mut self,
		parent: &B::Header,
		relay_parent: PHash,
		validation_data: &PersistedValidationData,
	) -> Option<ParachainCandidate<B>> {
		let (inherent_data, inherent_data_providers) =
			self.inherent_data(parent.hash(), validation_data, relay_parent).await?;

		let info = SlotInfo::new(
			inherent_data_providers.slot(),
			inherent_data_providers.timestamp(),
			inherent_data,
			self.slot_duration.slot_duration(),
			parent.clone(),
			// Set the block limit to 50% of the maximum PoV size.
			//
			// TODO: If we got benchmarking that includes the proof size,
			// we should be able to use the maximum pov size.
			Some((validation_data.max_pov_size / 2) as usize),
		);

		let res = self.aura_worker.lock().await.on_slot(info).await?;

		Some(ParachainCandidate { block: res.block, proof: res.storage_proof })
	}
}

/// Paramaters of [`build_aura_consensus`].
pub struct BuildAuraConsensusParams<PF, BI, CIDP, Client, BS, SO> {
	pub proposer_factory: PF,
	pub create_inherent_data_providers: CIDP,
	pub block_import: BI,
	pub para_client: Arc<Client>,
	pub backoff_authoring_blocks: Option<BS>,
	pub sync_oracle: SO,
	pub keystore: SyncCryptoStorePtr,
	pub force_authoring: bool,
	pub slot_duration: SlotDuration,
	pub telemetry: Option<TelemetryHandle>,
	pub block_proposal_slot_portion: SlotProportion,
	pub max_block_proposal_slot_portion: Option<SlotProportion>,
}
