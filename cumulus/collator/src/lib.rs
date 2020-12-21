// Copyright 2019 Parity Technologies (UK) Ltd.
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

use cumulus_network::WaitToAnnounce;
use cumulus_primitives::{
	inherents::{self, VALIDATION_DATA_IDENTIFIER},
	well_known_keys, InboundDownwardMessage, InboundHrmpMessage, OutboundHrmpMessage,
	ValidationData,
};
use cumulus_runtime::ParachainBlockData;

use sc_client_api::{BlockBackend, Finalizer, StateBackend, UsageProvider};
use sp_blockchain::HeaderBackend;
use sp_consensus::{
	BlockImport, BlockImportParams, BlockOrigin, BlockStatus, Environment, Error as ConsensusError,
	ForkChoiceStrategy, Proposal, Proposer, RecordProof,
};
use sp_core::traits::SpawnNamed;
use sp_inherents::{InherentData, InherentDataProviders};
use sp_runtime::{
	generic::BlockId,
	traits::{BlakeTwo256, Block as BlockT, Header as HeaderT},
};
use sp_state_machine::InspectState;

use polkadot_node_primitives::{Collation, CollationGenerationConfig};
use polkadot_node_subsystem::messages::{CollationGenerationMessage, CollatorProtocolMessage};
use polkadot_overseer::OverseerHandler;
use polkadot_primitives::v1::{
	Block as PBlock, BlockData, BlockNumber as PBlockNumber, CollatorPair, Hash as PHash, HeadData,
	Id as ParaId, PoV, UpwardMessage,
};
use polkadot_service::RuntimeApiCollection;

use codec::{Decode, Encode};

use log::{debug, error, info, trace};

use futures::prelude::*;

use std::{collections::BTreeMap, marker::PhantomData, sync::Arc, time::Duration};

use parking_lot::Mutex;

type TransactionFor<E, Block> =
	<<E as Environment<Block>>::Proposer as Proposer<Block>>::Transaction;

/// The implementation of the Cumulus `Collator`.
pub struct Collator<Block: BlockT, PF, BI, BS, Backend, PBackend, PClient, PBackend2> {
	para_id: ParaId,
	proposer_factory: Arc<Mutex<PF>>,
	_phantom: PhantomData<(Block, PBackend)>,
	inherent_data_providers: InherentDataProviders,
	block_import: Arc<Mutex<BI>>,
	block_status: Arc<BS>,
	wait_to_announce: Arc<Mutex<WaitToAnnounce<Block>>>,
	backend: Arc<Backend>,
	polkadot_client: Arc<PClient>,
	polkadot_backend: Arc<PBackend2>,
}

impl<Block: BlockT, PF, BI, BS, Backend, PBackend, PClient, PBackend2> Clone
	for Collator<Block, PF, BI, BS, Backend, PBackend, PClient, PBackend2>
{
	fn clone(&self) -> Self {
		Self {
			para_id: self.para_id.clone(),
			proposer_factory: self.proposer_factory.clone(),
			inherent_data_providers: self.inherent_data_providers.clone(),
			_phantom: PhantomData,
			block_import: self.block_import.clone(),
			block_status: self.block_status.clone(),
			wait_to_announce: self.wait_to_announce.clone(),
			backend: self.backend.clone(),
			polkadot_client: self.polkadot_client.clone(),
			polkadot_backend: self.polkadot_backend.clone(),
		}
	}
}

impl<Block, PF, BI, BS, Backend, PBackend, PApi, PClient, PBackend2>
	Collator<Block, PF, BI, BS, Backend, PBackend, PClient, PBackend2>
where
	Block: BlockT,
	PF: Environment<Block> + 'static + Send,
	PF::Proposer: Send,
	BI: BlockImport<
			Block,
			Error = ConsensusError,
			Transaction = <PF::Proposer as Proposer<Block>>::Transaction,
		> + Send
		+ Sync
		+ 'static,
	BS: BlockBackend<Block>,
	Backend: sc_client_api::Backend<Block> + 'static,
	PBackend: sc_client_api::Backend<PBlock> + 'static,
	PBackend::State: StateBackend<BlakeTwo256>,
	PApi: RuntimeApiCollection<StateBackend = PBackend::State>,
	PClient: polkadot_service::AbstractClient<PBlock, PBackend, Api = PApi> + 'static,
	PBackend2: sc_client_api::Backend<PBlock> + 'static,
	PBackend2::State: StateBackend<BlakeTwo256>,
{
	/// Create a new instance.
	fn new(
		para_id: ParaId,
		proposer_factory: PF,
		inherent_data_providers: InherentDataProviders,
		overseer_handler: OverseerHandler,
		block_import: BI,
		block_status: Arc<BS>,
		spawner: Arc<dyn SpawnNamed + Send + Sync>,
		announce_block: Arc<dyn Fn(Block::Hash, Vec<u8>) + Send + Sync>,
		backend: Arc<Backend>,
		polkadot_client: Arc<PClient>,
		polkadot_backend: Arc<PBackend2>,
	) -> Self {
		let wait_to_announce = Arc::new(Mutex::new(WaitToAnnounce::new(
			spawner,
			announce_block,
			overseer_handler,
		)));

		Self {
			para_id,
			proposer_factory: Arc::new(Mutex::new(proposer_factory)),
			inherent_data_providers,
			_phantom: PhantomData,
			block_import: Arc::new(Mutex::new(block_import)),
			block_status,
			wait_to_announce,
			backend,
			polkadot_client,
			polkadot_backend,
		}
	}

	/// Returns the whole contents of the downward message queue for the parachain we are collating
	/// for.
	///
	/// Returns `None` in case of an error.
	fn retrieve_dmq_contents(&self, relay_parent: PHash) -> Option<Vec<InboundDownwardMessage>> {
		self.polkadot_client
			.runtime_api()
			.dmq_contents_with_context(
				&BlockId::hash(relay_parent),
				sp_core::ExecutionContext::Importing,
				self.para_id,
			)
			.map_err(|e| {
				error!(
					target: "cumulus-collator",
					"An error occured during requesting the downward messages for {}: {:?}",
					relay_parent, e,
				);
			})
			.ok()
	}

	/// Returns channels contents for each inbound HRMP channel addressed to the parachain we are
	/// collating for.
	///
	/// Empty channels are also included.
	fn retrieve_all_inbound_hrmp_channel_contents(
		&self,
		relay_parent: PHash,
	) -> Option<BTreeMap<ParaId, Vec<InboundHrmpMessage>>> {
		self.polkadot_client
			.runtime_api()
			.inbound_hrmp_channels_contents_with_context(
				&BlockId::hash(relay_parent),
				sp_core::ExecutionContext::Importing,
				self.para_id,
			)
			.map_err(|e| {
				error!(
					target: "cumulus-collator",
					"An error occured during requesting the inbound HRMP messages for {}: {:?}",
					relay_parent, e,
				);
			})
			.ok()
	}

	/// Get the inherent data with validation function parameters injected
	fn inherent_data(
		&mut self,
		validation_data: &ValidationData,
		relay_parent: PHash,
	) -> Option<InherentData> {
		let mut inherent_data = self
			.inherent_data_providers
			.create_inherent_data()
			.map_err(|e| {
				error!(
					target: "cumulus-collator",
					"Failed to create inherent data: {:?}",
					e,
				)
			})
			.ok()?;

		let validation_data = {
			// TODO: Actual proof is to be created in the upcoming PRs.
			let relay_chain_state = sp_state_machine::StorageProof::empty();
			inherents::ValidationDataType {
				validation_data: validation_data.clone(),
				relay_chain_state,
			}
		};

		inherent_data
			.put_data(VALIDATION_DATA_IDENTIFIER, &validation_data)
			.map_err(|e| {
				error!(
					target: "cumulus-collator",
					"Failed to put validation function params into inherent data: {:?}",
					e,
				)
			})
			.ok()?;

		let message_ingestion_data = {
			let downward_messages = self.retrieve_dmq_contents(relay_parent)?;
			let horizontal_messages =
				self.retrieve_all_inbound_hrmp_channel_contents(relay_parent)?;

			inherents::MessageIngestionType {
				downward_messages,
				horizontal_messages,
			}
		};

		inherent_data
			.put_data(
				inherents::MESSAGE_INGESTION_IDENTIFIER,
				&message_ingestion_data,
			)
			.map_err(|e| {
				error!(
					target: "cumulus-collator",
					"Failed to put downward messages into inherent data: {:?}",
					e,
				)
			})
			.ok()?;

		Some(inherent_data)
	}

	/// Checks the status of the given block hash in the Parachain.
	///
	/// Returns `true` if the block could be found and is good to be build on.
	fn check_block_status(&self, hash: Block::Hash) -> bool {
		match self.block_status.block_status(&BlockId::Hash(hash)) {
			Ok(BlockStatus::Queued) => {
				debug!(
					target: "cumulus-collator",
					"Skipping candidate production, because block `{:?}` is still queued for import.", hash,
				);
				false
			}
			Ok(BlockStatus::InChainWithState) => true,
			Ok(BlockStatus::InChainPruned) => {
				error!(
					target: "cumulus-collator",
					"Skipping candidate production, because block `{:?}` is already pruned!", hash,
				);
				false
			}
			Ok(BlockStatus::KnownBad) => {
				error!(
					target: "cumulus-collator",
					"Block `{}` is tagged as known bad and is included in the relay chain! Skipping candidate production!", hash,
				);
				false
			}
			Ok(BlockStatus::Unknown) => {
				debug!(
					target: "cumulus-collator",
					"Skipping candidate production, because block `{:?}` is unknown.", hash,
				);
				false
			}
			Err(e) => {
				error!(target: "cumulus-collator", "Failed to get block status of `{:?}`: {:?}", hash, e);
				false
			}
		}
	}

	fn build_collation(
		&mut self,
		block: ParachainBlockData<Block>,
		block_hash: Block::Hash,
		relay_block_number: PBlockNumber,
	) -> Option<Collation> {
		let block_data = BlockData(block.encode());
		let header = block.into_header();
		let head_data = HeadData(header.encode());

		let state = match self.backend.state_at(BlockId::Hash(block_hash)) {
			Ok(state) => state,
			Err(e) => {
				error!(target: "cumulus-collator", "Failed to get state of the freshly built block: {:?}", e);
				return None;
			}
		};

		state.inspect_state(|| {
			let upward_messages = sp_io::storage::get(well_known_keys::UPWARD_MESSAGES);
			let upward_messages = match upward_messages.map(|v| Vec::<UpwardMessage>::decode(&mut &v[..])) {
				Some(Ok(msgs)) => msgs,
				Some(Err(e)) => {
					error!(target: "cumulus-collator", "Failed to decode upward messages from the build block: {:?}", e);
					return None
				},
				None => Vec::new(),
			};

			let new_validation_code = sp_io::storage::get(well_known_keys::NEW_VALIDATION_CODE);

			let processed_downward_messages = sp_io::storage::get(well_known_keys::PROCESSED_DOWNWARD_MESSAGES);
			let processed_downward_messages = match processed_downward_messages
				.map(|v| u32::decode(&mut &v[..]))
			{
				Some(Ok(processed_cnt)) => processed_cnt,
				Some(Err(e)) => {
					error!(
						target: "cumulus-collator",
						"Failed to decode the count of processed downward messages: {:?}",
						e
					);
					return None
				}
				None => 0,
			};

			let horizontal_messages = sp_io::storage::get(well_known_keys::HRMP_OUTBOUND_MESSAGES);
			let horizontal_messages = match horizontal_messages
				.map(|v| Vec::<OutboundHrmpMessage>::decode(&mut &v[..]))
			{
				Some(Ok(horizontal_messages)) => horizontal_messages,
				Some(Err(e)) => {
					error!(
						target: "cumulus-collator",
						"Failed to decode the horizontal messages: {:?}",
						e
					);
					return None
				}
				None => Vec::new(),
			};

			let hrmp_watermark = sp_io::storage::get(well_known_keys::HRMP_WATERMARK);
			let hrmp_watermark = match hrmp_watermark.map(|v| PBlockNumber::decode(&mut &v[..])) {
				Some(Ok(hrmp_watermark)) => hrmp_watermark,
				Some(Err(e)) => {
					error!(
						target: "cumulus-collator",
						"Failed to decode the HRMP watermark: {:?}",
						e
					);
					return None
				}
				None => {
					// If the runtime didn't set `HRMP_WATERMARK`, then it means no messages were
					// supplied via the message ingestion inherent. Assuming that the PVF/runtime
					// checks that legitly there are no pending messages we can therefore move the
					// watermark up to the relay-block number.
					relay_block_number
				}
			};

			Some(Collation {
				upward_messages,
				new_validation_code: new_validation_code.map(Into::into),
				head_data,
				proof_of_validity: PoV { block_data },
				processed_downward_messages,
				horizontal_messages,
				hrmp_watermark,
			})
		})
	}

	async fn produce_candidate(
		mut self,
		relay_parent: PHash,
		validation_data: ValidationData,
	) -> Option<Collation> {
		trace!(target: "cumulus-collator", "Producing candidate");

		let last_head =
			match Block::Header::decode(&mut &validation_data.persisted.parent_head.0[..]) {
				Ok(x) => x,
				Err(e) => {
					error!(target: "cumulus-collator", "Could not decode the head data: {:?}", e);
					return None;
				}
			};

		let last_head_hash = last_head.hash();
		if !self.check_block_status(last_head_hash) {
			return None;
		}

		info!(
			target: "cumulus-collator",
			"Starting collation for relay parent {:?} on parent {:?}.",
			relay_parent,
			last_head_hash,
		);

		let proposer_future = self.proposer_factory.lock().init(&last_head);

		let proposer = proposer_future
			.await
			.map_err(|e| {
				error!(
					target: "cumulus-collator",
					"Could not create proposer: {:?}",
					e,
				)
			})
			.ok()?;

		let inherent_data = self.inherent_data(&validation_data, relay_parent)?;

		let Proposal {
			block,
			storage_changes,
			proof,
		} = proposer
			.propose(
				inherent_data,
				Default::default(),
				//TODO: Fix this.
				Duration::from_millis(500),
				RecordProof::Yes,
			)
			.await
			.map_err(|e| {
				error!(
					target: "cumulus-collator",
					"Proposing failed: {:?}",
					e,
				)
			})
			.ok()?;

		let proof = match proof {
			Some(proof) => proof,
			None => {
				error!(
					target: "cumulus-collator",
					"Proposer did not return the requested proof.",
				);

				return None;
			}
		};

		let (header, extrinsics) = block.deconstruct();
		let block_hash = header.hash();

		// Create the parachain block data for the validators.
		let b = ParachainBlockData::<Block>::new(header.clone(), extrinsics, proof);

		let mut block_import_params = BlockImportParams::new(BlockOrigin::Own, header);
		block_import_params.body = Some(b.extrinsics().to_vec());
		// Best block is determined by the relay chain.
		block_import_params.fork_choice = Some(ForkChoiceStrategy::Custom(false));
		block_import_params.storage_changes = Some(storage_changes);

		if let Err(err) = self
			.block_import
			.lock()
			.import_block(block_import_params, Default::default())
		{
			error!(
				target: "cumulus-collator",
				"Error importing build block (at {:?}): {:?}",
				b.header().parent_hash(),
				err,
			);

			return None;
		}

		let collation =
			self.build_collation(b, block_hash, validation_data.persisted.block_number)?;
		let pov_hash = collation.proof_of_validity.hash();

		self.wait_to_announce
			.lock()
			.wait_to_announce(block_hash, pov_hash);

		info!(
			target: "cumulus-collator",
			"Produced proof-of-validity candidate {:?} from block {:?}.",
			pov_hash,
			block_hash,
		);

		Some(collation)
	}
}

/// Parameters for [`start_collator`].
pub struct StartCollatorParams<Block: BlockT, PF, BI, Backend, Client, BS, Spawner, PClient, PBackend> {
	pub proposer_factory: PF,
	pub inherent_data_providers: InherentDataProviders,
	pub backend: Arc<Backend>,
	pub block_import: BI,
	pub block_status: Arc<BS>,
	pub client: Arc<Client>,
	pub announce_block: Arc<dyn Fn(Block::Hash, Vec<u8>) + Send + Sync>,
	pub overseer_handler: OverseerHandler,
	pub spawner: Spawner,
	pub para_id: ParaId,
	pub key: CollatorPair,
	pub polkadot_client: Arc<PClient>,
	pub polkadot_backend: Arc<PBackend>,
}

pub async fn start_collator<
	Block: BlockT,
	PF,
	BI,
	Backend,
	Client,
	BS,
	Spawner,
	PClient,
	PBackend,
	PBackend2,
	PApi,
>(
	StartCollatorParams {
		proposer_factory,
		inherent_data_providers,
		backend,
		block_import,
		block_status,
		client,
		announce_block,
		mut overseer_handler,
		spawner,
		para_id,
		key,
		polkadot_client,
		polkadot_backend,
	}: StartCollatorParams<Block, PF, BI, Backend, Client, BS, Spawner, PClient, PBackend2>,
) -> Result<(), String>
where
	PF: Environment<Block> + Send + 'static,
	BI: BlockImport<Block, Error = sp_consensus::Error, Transaction = TransactionFor<PF, Block>>
		+ Send
		+ Sync
		+ 'static,
	Backend: sc_client_api::Backend<Block> + 'static,
	Client: Finalizer<Block, Backend>
		+ UsageProvider<Block>
		+ HeaderBackend<Block>
		+ Send
		+ Sync
		+ BlockBackend<Block>
		+ 'static,
	for<'a> &'a Client: BlockImport<Block>,
	BS: BlockBackend<Block> + Send + Sync + 'static,
	Spawner: SpawnNamed + Clone + Send + Sync + 'static,
	PBackend: sc_client_api::Backend<PBlock> + 'static,
	PBackend::State: StateBackend<BlakeTwo256>,
	PApi: RuntimeApiCollection<StateBackend = PBackend::State>,
	PClient: polkadot_service::AbstractClient<PBlock, PBackend, Api = PApi> + 'static,
	PBackend2: sc_client_api::Backend<PBlock> + 'static,
	PBackend2::State: StateBackend<BlakeTwo256>,
{
	let follow = match cumulus_consensus::follow_polkadot(
		para_id,
		client,
		polkadot_client.clone(),
		announce_block.clone(),
	) {
		Ok(follow) => follow,
		Err(e) => return Err(format!("Could not start following polkadot: {:?}", e)),
	};

	spawner.spawn("cumulus-follow-polkadot", follow.map(|_| ()).boxed());

	let collator = Collator::new(
		para_id,
		proposer_factory,
		inherent_data_providers,
		overseer_handler.clone(),
		block_import,
		block_status,
		Arc::new(spawner),
		announce_block,
		backend,
		polkadot_client,
		polkadot_backend,
	);

	let config = CollationGenerationConfig {
		key,
		para_id,
		collator: Box::new(move |relay_parent, validation_data| {
			let collator = collator.clone();
			collator
				.produce_candidate(relay_parent, validation_data.clone())
				.boxed()
		}),
	};

	overseer_handler
		.send_msg(CollationGenerationMessage::Initialize(config))
		.await;

	overseer_handler
		.send_msg(CollatorProtocolMessage::CollateOn(para_id))
		.await;

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{pin::Pin, time::Duration};

	use sc_block_builder::BlockBuilderProvider;
	use sp_core::{testing::TaskExecutor, Pair};
	use sp_inherents::InherentData;
	use sp_runtime::traits::DigestFor;

	use cumulus_test_client::{
		generate_block_inherents, Client, DefaultTestClientBuilderExt, TestClientBuilder,
		TestClientBuilderExt,
	};
	use cumulus_test_runtime::{Block, Header};

	use polkadot_node_subsystem::messages::CollationGenerationMessage;
	use polkadot_node_subsystem_test_helpers::ForwardSubsystem;
	use polkadot_overseer::{AllSubsystems, Overseer};

	use futures::{channel::mpsc, executor::block_on, future};

	#[derive(Debug)]
	struct Error;

	impl From<sp_consensus::Error> for Error {
		fn from(_: sp_consensus::Error) -> Self {
			unimplemented!("Not required in tests")
		}
	}

	struct DummyFactory(Arc<Client>);

	impl Environment<Block> for DummyFactory {
		type Proposer = DummyProposer;
		type Error = Error;
		type CreateProposer = Pin<
			Box<dyn Future<Output = Result<Self::Proposer, Self::Error>> + Send + Unpin + 'static>,
		>;

		fn init(&mut self, header: &Header) -> Self::CreateProposer {
			Box::pin(future::ready(Ok(DummyProposer {
				client: self.0.clone(),
				header: header.clone(),
			})))
		}
	}

	struct DummyProposer {
		client: Arc<Client>,
		header: Header,
	}

	impl Proposer<Block> for DummyProposer {
		type Error = Error;
		type Proposal = future::Ready<Result<Proposal<Block, Self::Transaction>, Error>>;
		type Transaction = sc_client_api::TransactionFor<cumulus_test_client::Backend, Block>;

		fn propose(
			self,
			_: InherentData,
			digest: DigestFor<Block>,
			_: Duration,
			record_proof: RecordProof,
		) -> Self::Proposal {
			let block_id = BlockId::Hash(self.header.hash());
			let mut builder = self
				.client
				.new_block_at(&block_id, digest, record_proof.yes())
				.expect("Initializes new block");

			generate_block_inherents(&*self.client, None)
				.into_iter()
				.for_each(|e| builder.push(e).expect("Pushes an inherent"));

			let (block, storage_changes, proof) =
				builder.build().expect("Creates block").into_inner();

			future::ready(Ok(Proposal {
				block,
				storage_changes,
				proof,
			}))
		}
	}

	#[test]
	fn collates_produces_a_block() {
		let _ = env_logger::try_init();

		let spawner = TaskExecutor::new();
		let para_id = ParaId::from(100);
		let announce_block = |_, _| ();
		let client_builder = TestClientBuilder::new();
		let backend = client_builder.backend();
		let client = Arc::new(client_builder.build());
		let header = client.header(&BlockId::Number(0)).unwrap().unwrap();

		let (sub_tx, sub_rx) = mpsc::channel(64);

		let all_subsystems =
			AllSubsystems::<()>::dummy().replace_collation_generation(ForwardSubsystem(sub_tx));
		let (overseer, handler) = Overseer::new(Vec::new(), all_subsystems, None, spawner.clone())
			.expect("Creates overseer");

		spawner.spawn("overseer", overseer.run().then(|_| async { () }).boxed());

		let (polkadot_client, polkadot_backend, relay_parent) = {
			// Create a polkadot client with a block imported.
			use polkadot_test_client::{
				ClientBlockImportExt as _, DefaultTestClientBuilderExt as _,
				InitPolkadotBlockBuilder as _, TestClientBuilderExt as _,
			};

			let client_builder = polkadot_test_client::TestClientBuilder::new();
			let polkadot_backend = client_builder.backend();
			let mut client = client_builder.build();
			let block_builder = client.init_polkadot_block_builder();
			let block = block_builder.build().expect("Finalizes the block").block;
			let hash = block.header().hash();
			client
				.import_as_best(BlockOrigin::Own, block)
				.expect("Imports the block");
			(client, polkadot_backend, hash)
		};

		let collator_start =
			start_collator::<_, _, _, _, _, _, _, _, polkadot_service::FullBackend, _, _>(
				StartCollatorParams {
					proposer_factory: DummyFactory(client.clone()),
					inherent_data_providers: Default::default(),
					backend,
					block_import: client.clone(),
					block_status: client.clone(),
					client: client.clone(),
					announce_block: Arc::new(announce_block),
					overseer_handler: handler,
					spawner,
					para_id,
					key: CollatorPair::generate().0,
					polkadot_client: Arc::new(polkadot_client),
					polkadot_backend,
				},
			);
		block_on(collator_start).expect("Should start collator");

		let msg = block_on(sub_rx.into_future())
			.0
			.expect("message should be send by `start_collator` above.");

		let config = match msg {
			CollationGenerationMessage::Initialize(config) => config,
		};

		let mut validation_data = ValidationData::default();
		validation_data.persisted.parent_head = header.encode().into();

		let collation = block_on((config.collator)(relay_parent, &validation_data))
			.expect("Collation is build");

		let block_data = collation.proof_of_validity.block_data;

		let block = Block::decode(&mut &block_data.0[..]).expect("Is a valid block");

		assert_eq!(1, *block.header().number());
	}
}
