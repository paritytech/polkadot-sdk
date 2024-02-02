// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Cumulus Collator implementation for Substrate.

use cumulus_primitives_core::{
	relay_chain::Hash as PHash, CollectCollationInfo, PersistedValidationData,
};

use sc_client_api::BlockBackend;
use sp_api::ProvideRuntimeApi;
use sp_core::traits::SpawnNamed;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};

use cumulus_client_consensus_common::ParachainConsensus;
use polkadot_node_primitives::{CollationGenerationConfig, CollationResult, MaybeCompressedPoV};
use polkadot_node_subsystem::messages::{CollationGenerationMessage, CollatorProtocolMessage};
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::{CollatorPair, Id as ParaId};

use codec::{Decode, Encode};
use futures::prelude::*;
use std::sync::Arc;

use crate::service::CollatorService;

pub mod service;

/// The logging target.
const LOG_TARGET: &str = "cumulus-collator";

/// The implementation of the Cumulus `Collator`.
///
/// Note that this implementation is soon to be deprecated and removed, and it is suggested to
/// directly use the [`CollatorService`] instead, so consensus engine implementations
/// live at the top level.
pub struct Collator<Block: BlockT, BS, RA> {
	service: CollatorService<Block, BS, RA>,
	parachain_consensus: Box<dyn ParachainConsensus<Block>>,
}

impl<Block: BlockT, BS, RA> Clone for Collator<Block, BS, RA> {
	fn clone(&self) -> Self {
		Collator {
			service: self.service.clone(),
			parachain_consensus: self.parachain_consensus.clone(),
		}
	}
}

impl<Block, BS, RA> Collator<Block, BS, RA>
where
	Block: BlockT,
	BS: BlockBackend<Block>,
	RA: ProvideRuntimeApi<Block>,
	RA::Api: CollectCollationInfo<Block>,
{
	/// Create a new instance.
	fn new(
		collator_service: CollatorService<Block, BS, RA>,
		parachain_consensus: Box<dyn ParachainConsensus<Block>>,
	) -> Self {
		Self { service: collator_service, parachain_consensus }
	}

	async fn produce_candidate(
		mut self,
		relay_parent: PHash,
		validation_data: PersistedValidationData,
	) -> Option<CollationResult> {
		tracing::trace!(
			target: LOG_TARGET,
			relay_parent = ?relay_parent,
			"Producing candidate",
		);

		let last_head = match Block::Header::decode(&mut &validation_data.parent_head.0[..]) {
			Ok(x) => x,
			Err(e) => {
				tracing::error!(
					target: LOG_TARGET,
					error = ?e,
					"Could not decode the head data."
				);
				return None
			},
		};

		let last_head_hash = last_head.hash();
		if !self.service.check_block_status(last_head_hash, &last_head) {
			return None
		}

		tracing::info!(
			target: LOG_TARGET,
			relay_parent = ?relay_parent,
			at = ?last_head_hash,
			"Starting collation.",
		);

		let candidate = self
			.parachain_consensus
			.produce_candidate(&last_head, relay_parent, &validation_data)
			.await?;

		let block_hash = candidate.block.header().hash();

		let (collation, b) = self.service.build_collation(&last_head, block_hash, candidate)?;

		tracing::info!(
			target: LOG_TARGET,
			"PoV size {{ header: {}kb, extrinsics: {}kb, storage_proof: {}kb }}",
			b.header().encode().len() as f64 / 1024f64,
			b.extrinsics().encode().len() as f64 / 1024f64,
			b.storage_proof().encode().len() as f64 / 1024f64,
		);

		if let MaybeCompressedPoV::Compressed(ref pov) = collation.proof_of_validity {
			tracing::info!(
				target: LOG_TARGET,
				"Compressed PoV size: {}kb",
				pov.block_data.0.len() as f64 / 1024f64,
			);
		}

		let result_sender = self.service.announce_with_barrier(block_hash);

		tracing::info!(target: LOG_TARGET, ?block_hash, "Produced proof-of-validity candidate.",);

		Some(CollationResult { collation, result_sender: Some(result_sender) })
	}
}

/// Relay-chain-driven collators are those whose block production is driven purely
/// by new relay chain blocks and the most recently included parachain blocks
/// within them.
///
/// This method of driving collators is not suited to anything but the most simple parachain
/// consensus mechanisms, and this module may soon be deprecated.
pub mod relay_chain_driven {
	use futures::{
		channel::{mpsc, oneshot},
		prelude::*,
	};
	use polkadot_node_primitives::{CollationGenerationConfig, CollationResult};
	use polkadot_node_subsystem::messages::{CollationGenerationMessage, CollatorProtocolMessage};
	use polkadot_overseer::Handle as OverseerHandle;
	use polkadot_primitives::{CollatorPair, Id as ParaId};

	use cumulus_primitives_core::{relay_chain::Hash as PHash, PersistedValidationData};

	/// A request to author a collation, based on the advancement of the relay chain.
	///
	/// See the module docs for more info on relay-chain-driven collators.
	pub struct CollationRequest {
		relay_parent: PHash,
		pvd: PersistedValidationData,
		sender: oneshot::Sender<Option<CollationResult>>,
	}

	impl CollationRequest {
		/// Get the relay parent of the collation request.
		pub fn relay_parent(&self) -> &PHash {
			&self.relay_parent
		}

		/// Get the [`PersistedValidationData`] for the request.
		pub fn persisted_validation_data(&self) -> &PersistedValidationData {
			&self.pvd
		}

		/// Complete the request with a collation, if any.
		pub fn complete(self, collation: Option<CollationResult>) {
			let _ = self.sender.send(collation);
		}
	}

	/// Initialize the collator with Polkadot's collation-generation
	/// subsystem, returning a stream of collation requests to handle.
	pub async fn init(
		key: CollatorPair,
		para_id: ParaId,
		overseer_handle: OverseerHandle,
	) -> mpsc::Receiver<CollationRequest> {
		let mut overseer_handle = overseer_handle;

		let (stream_tx, stream_rx) = mpsc::channel(0);
		let config = CollationGenerationConfig {
			key,
			para_id,
			collator: Some(Box::new(move |relay_parent, validation_data| {
				// Cloning the channel on each usage effectively makes the channel
				// unbounded. The channel is actually bounded by the block production
				// and consensus systems of Polkadot, which limits the amount of possible
				// blocks.
				let mut stream_tx = stream_tx.clone();
				let validation_data = validation_data.clone();
				Box::pin(async move {
					let (this_tx, this_rx) = oneshot::channel();
					let request =
						CollationRequest { relay_parent, pvd: validation_data, sender: this_tx };

					if stream_tx.send(request).await.is_err() {
						return None
					}

					this_rx.await.ok().flatten()
				})
			})),
		};

		overseer_handle
			.send_msg(CollationGenerationMessage::Initialize(config), "StartCollator")
			.await;

		overseer_handle
			.send_msg(CollatorProtocolMessage::CollateOn(para_id), "StartCollator")
			.await;

		stream_rx
	}
}

/// Initialize the collation-related subsystems on the relay-chain side.
///
/// This must be done prior to collation, and does not set up any callback for collation.
/// For callback-driven collators, use the [`relay_chain_driven`] module.
pub async fn initialize_collator_subsystems(
	overseer_handle: &mut OverseerHandle,
	key: CollatorPair,
	para_id: ParaId,
	reinitialize: bool,
) {
	let config = CollationGenerationConfig { key, para_id, collator: None };

	if reinitialize {
		overseer_handle
			.send_msg(CollationGenerationMessage::Reinitialize(config), "StartCollator")
			.await;
	} else {
		overseer_handle
			.send_msg(CollationGenerationMessage::Initialize(config), "StartCollator")
			.await;
	}

	overseer_handle
		.send_msg(CollatorProtocolMessage::CollateOn(para_id), "StartCollator")
		.await;
}

/// Parameters for [`start_collator`].
pub struct StartCollatorParams<Block: BlockT, RA, BS, Spawner> {
	pub para_id: ParaId,
	pub runtime_api: Arc<RA>,
	pub block_status: Arc<BS>,
	pub announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	pub overseer_handle: OverseerHandle,
	pub spawner: Spawner,
	pub key: CollatorPair,
	pub parachain_consensus: Box<dyn ParachainConsensus<Block>>,
}

/// Start the collator.
#[deprecated = "Collators should run consensus futures which handle this logic internally"]
pub async fn start_collator<Block, RA, BS, Spawner>(
	params: StartCollatorParams<Block, RA, BS, Spawner>,
) where
	Block: BlockT,
	BS: BlockBackend<Block> + Send + Sync + 'static,
	Spawner: SpawnNamed + Clone + Send + Sync + 'static,
	RA: ProvideRuntimeApi<Block> + Send + Sync + 'static,
	RA::Api: CollectCollationInfo<Block>,
{
	// This never needed to be asynchronous, but shouldn't be changed due to backcompat.
	#[allow(deprecated)]
	start_collator_sync(params);
}

/// Start the collator in a synchronous function.
#[deprecated = "Collators should run consensus futures which handle this logic internally"]
pub fn start_collator_sync<Block, RA, BS, Spawner>(
	StartCollatorParams {
		para_id,
		block_status,
		announce_block,
		overseer_handle,
		spawner,
		key,
		parachain_consensus,
		runtime_api,
	}: StartCollatorParams<Block, RA, BS, Spawner>,
) where
	Block: BlockT,
	BS: BlockBackend<Block> + Send + Sync + 'static,
	Spawner: SpawnNamed + Clone + Send + Sync + 'static,
	RA: ProvideRuntimeApi<Block> + Send + Sync + 'static,
	RA::Api: CollectCollationInfo<Block>,
{
	let collator_service =
		CollatorService::new(block_status, Arc::new(spawner.clone()), announce_block, runtime_api);

	let collator = Collator::new(collator_service, parachain_consensus);

	let collation_future = Box::pin(async move {
		let mut request_stream = relay_chain_driven::init(key, para_id, overseer_handle).await;
		while let Some(request) = request_stream.next().await {
			let collation = collator
				.clone()
				.produce_candidate(
					*request.relay_parent(),
					request.persisted_validation_data().clone(),
				)
				.await;

			request.complete(collation);
		}
	});

	spawner.spawn("cumulus-relay-driven-collator", None, collation_future);
}

#[cfg(test)]
mod tests {
	use super::*;
	use async_trait::async_trait;
	use cumulus_client_consensus_common::ParachainCandidate;
	use cumulus_primitives_core::ParachainBlockData;
	use cumulus_test_client::{
		Client, ClientBlockImportExt, DefaultTestClientBuilderExt, InitBlockBuilder,
		TestClientBuilder, TestClientBuilderExt,
	};
	use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
	use cumulus_test_runtime::{Block, Header};
	use futures::{channel::mpsc, executor::block_on, StreamExt};
	use polkadot_node_primitives::CollationGenerationConfig;
	use polkadot_node_subsystem::messages::CollationGenerationMessage;
	use polkadot_node_subsystem_test_helpers::ForwardSubsystem;
	use polkadot_overseer::{dummy::dummy_overseer_builder, HeadSupportsParachains};
	use polkadot_primitives::HeadData;
	use sp_consensus::BlockOrigin;
	use sp_core::{testing::TaskExecutor, Pair};
	use sp_runtime::traits::BlakeTwo256;
	use sp_state_machine::Backend;

	struct AlwaysSupportsParachains;

	#[async_trait]
	impl HeadSupportsParachains for AlwaysSupportsParachains {
		async fn head_supports_parachains(&self, _head: &PHash) -> bool {
			true
		}
	}

	#[derive(Clone)]
	struct DummyParachainConsensus {
		client: Arc<Client>,
	}

	#[async_trait::async_trait]
	impl ParachainConsensus<Block> for DummyParachainConsensus {
		async fn produce_candidate(
			&mut self,
			parent: &Header,
			_: PHash,
			validation_data: &PersistedValidationData,
		) -> Option<ParachainCandidate<Block>> {
			let mut sproof = RelayStateSproofBuilder::default();
			sproof.included_para_head = Some(HeadData(parent.encode()));
			sproof.para_id = cumulus_test_runtime::PARACHAIN_ID.into();

			let builder = self.client.init_block_builder_at(
				parent.hash(),
				Some(validation_data.clone()),
				sproof,
			);

			let (block, _, proof) = builder.build().expect("Creates block").into_inner();

			self.client
				.import(BlockOrigin::Own, block.clone())
				.await
				.expect("Imports the block");

			Some(ParachainCandidate { block, proof: proof.expect("Proof is returned") })
		}
	}

	#[test]
	fn collates_produces_a_block_and_storage_proof_does_not_contains_code() {
		sp_tracing::try_init_simple();

		let spawner = TaskExecutor::new();
		let para_id = ParaId::from(100);
		let announce_block = |_, _| ();
		let client = Arc::new(TestClientBuilder::new().build());
		let header = client.header(client.chain_info().genesis_hash).unwrap().unwrap();

		let (sub_tx, sub_rx) = mpsc::channel(64);

		let (overseer, handle) =
			dummy_overseer_builder(spawner.clone(), AlwaysSupportsParachains, None)
				.expect("Creates overseer builder")
				.replace_collation_generation(|_| ForwardSubsystem(sub_tx))
				.build()
				.expect("Builds overseer");

		spawner.spawn("overseer", None, overseer.run().then(|_| async {}).boxed());

		#[allow(deprecated)]
		let collator_start = start_collator(StartCollatorParams {
			runtime_api: client.clone(),
			block_status: client.clone(),
			announce_block: Arc::new(announce_block),
			overseer_handle: OverseerHandle::new(handle),
			spawner,
			para_id,
			key: CollatorPair::generate().0,
			parachain_consensus: Box::new(DummyParachainConsensus { client }),
		});
		block_on(collator_start);

		let msg = block_on(sub_rx.into_future())
			.0
			.expect("message should be send by `start_collator` above.");

		let collator_fn = match msg {
			CollationGenerationMessage::Initialize(CollationGenerationConfig {
				collator: Some(c),
				..
			}) => c,
			_ => panic!("unexpected message or no collator fn"),
		};

		let validation_data =
			PersistedValidationData { parent_head: header.encode().into(), ..Default::default() };
		let relay_parent = Default::default();

		let collation = block_on(collator_fn(relay_parent, &validation_data))
			.expect("Collation is build")
			.collation;

		let pov = collation.proof_of_validity.into_compressed();

		let decompressed =
			sp_maybe_compressed_blob::decompress(&pov.block_data.0, 1024 * 1024 * 10).unwrap();

		let block =
			ParachainBlockData::<Block>::decode(&mut &decompressed[..]).expect("Is a valid block");

		assert_eq!(1, *block.header().number());

		// Ensure that we did not include `:code` in the proof.
		let proof = block.storage_proof();

		let backend = sp_state_machine::create_proof_check_backend::<BlakeTwo256>(
			*header.state_root(),
			proof.to_storage_proof::<BlakeTwo256>(None).unwrap().0,
		)
		.unwrap();

		// Should return an error, as it was not included while building the proof.
		assert!(backend
			.storage(sp_core::storage::well_known_keys::CODE)
			.unwrap_err()
			.contains("Trie lookup error: Database missing expected key"));
	}
}
