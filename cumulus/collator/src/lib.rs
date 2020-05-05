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

use cumulus_primitives::{
	inherents::VALIDATION_FUNCTION_PARAMS_IDENTIFIER as VFP_IDENT,
	validation_function_params::ValidationFunctionParams,
};
use cumulus_runtime::ParachainBlockData;

use sp_consensus::{
	BlockImport, BlockImportParams, BlockOrigin, Environment, Error as ConsensusError,
	ForkChoiceStrategy, Proposal, Proposer, RecordProof,
};
use sp_inherents::{InherentData, InherentDataProviders};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, HashFor};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sc_client_api::{StateBackend, UsageProvider, Finalizer, BlockchainEvents};

use polkadot_collator::{
	BuildParachainContext, InvalidHead, Network as CollatorNetwork, ParachainContext,
	RuntimeApiCollection,
};
use polkadot_primitives::{
	parachain::{self, BlockData, GlobalValidationSchedule, LocalValidationData, Id as ParaId},
	Block as PBlock, Hash as PHash,
};

use codec::{Decode, Encode};

use log::{error, trace};

use futures::{task::Spawn, Future, future, FutureExt};

use std::{fmt::Debug, marker::PhantomData, sync::Arc, time::Duration, pin::Pin};

use parking_lot::Mutex;

/// The head data of the parachain, stored in the relay chain.
#[derive(Decode, Encode, Debug)]
struct HeadData<Block: BlockT> {
	header: Block::Header,
}

/// The implementation of the Cumulus `Collator`.
pub struct Collator<Block, PF, BI> {
	proposer_factory: Arc<Mutex<PF>>,
	_phantom: PhantomData<Block>,
	inherent_data_providers: InherentDataProviders,
	collator_network: Arc<dyn CollatorNetwork>,
	block_import: Arc<Mutex<BI>>,
}

impl<Block, PF, BI> Collator<Block, PF, BI> {
	/// Create a new instance.
	fn new(
		proposer_factory: PF,
		inherent_data_providers: InherentDataProviders,
		collator_network: impl CollatorNetwork + Clone + 'static,
		block_import: BI,
	) -> Self {
		Self {
			proposer_factory: Arc::new(Mutex::new(proposer_factory)),
			inherent_data_providers,
			_phantom: PhantomData,
			collator_network: Arc::new(collator_network),
			block_import: Arc::new(Mutex::new(block_import)),
		}
	}

	/// Get the inherent data with validation function parameters injected
	fn inherent_data(
		inherent_providers: InherentDataProviders,
		global_validation: GlobalValidationSchedule,
		local_validation: LocalValidationData,
	) -> Result<InherentData, InvalidHead> {
		let mut inherent_data = inherent_providers
			.create_inherent_data()
			.map_err(|e| {
				error!(
					target: "cumulus-collator",
					"Failed to create inherent data: {:?}",
					e,
				);
				InvalidHead
			})?;

		inherent_data.put_data(
			VFP_IDENT,
			&ValidationFunctionParams::from((global_validation, local_validation))
		).map_err(|e| {
			error!(
				target: "cumulus-collator",
				"Failed to inject validation function params into inherents: {:?}",
				e,
			);
			InvalidHead
		})?;

		Ok(inherent_data)
	}
}

impl<Block, PF, BI> Clone for Collator<Block, PF, BI> {
	fn clone(&self) -> Self {
		Self {
			proposer_factory: self.proposer_factory.clone(),
			inherent_data_providers: self.inherent_data_providers.clone(),
			_phantom: PhantomData,
			collator_network: self.collator_network.clone(),
			block_import: self.block_import.clone(),
		}
	}
}

impl<Block, PF, BI> ParachainContext for Collator<Block, PF, BI>
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
{
	type ProduceCandidate = Pin<Box<
		dyn Future<Output=Result<(BlockData, parachain::HeadData), InvalidHead>>
		+ Send,
	>>;

	fn produce_candidate(
		&mut self,
		_relay_chain_parent: PHash,
		global_validation: GlobalValidationSchedule,
		local_validation: LocalValidationData,
	) -> Self::ProduceCandidate {
		let factory = self.proposer_factory.clone();
		let inherent_providers = self.inherent_data_providers.clone();
		let block_import = self.block_import.clone();

		trace!(target: "cumulus-collator", "Producing candidate");

		let last_head = match HeadData::<Block>::decode(&mut &local_validation.parent_head.0[..]) {
			Ok(x) => x,
			Err(e) => {
				error!(target: "cumulus-collator", "Could not decode the head data: {:?}", e);
				return Box::pin(future::ready(Err(InvalidHead)));
			}
		};

		let proposer_future = factory
			.lock()
			.init(&last_head.header);

		Box::pin(async move {
			let parent_state_root = *last_head.header.state_root();

			let mut proposer = proposer_future
				.await
				.map_err(|e| {
					error!(
						target: "cumulus-collator",
						"Could not create proposer: {:?}",
						e,
					);
					InvalidHead
				})?;

			let inherent_data = Self::inherent_data(inherent_providers, global_validation, local_validation)?;

			let Proposal {
				block,
				storage_changes,
				proof,
			} = proposer
				.propose(
					inherent_data,
					Default::default(),
					//TODO: Fix this.
					Duration::from_secs(6),
					RecordProof::Yes,
				)
				.await
				.map_err(|e| {
					error!(
						target: "cumulus-collator",
						"Proposing failed: {:?}",
						e,
					);
					InvalidHead
				})?;

			let proof = proof
				.ok_or_else(|| {
					error!(
						target: "cumulus-collator",
						"Proposer did not return the requested proof.",
					);
					InvalidHead
				})?;

			let (header, extrinsics) = block.deconstruct();

			// Create the parachain block data for the validators.
			let b = ParachainBlockData::<Block>::new(
				header.clone(),
				extrinsics,
				proof.iter_nodes().collect(),
				parent_state_root,
			);

			let mut block_import_params = BlockImportParams::new(BlockOrigin::Own, header);
			block_import_params.body = Some(b.extrinsics().to_vec());
			block_import_params.fork_choice = Some(ForkChoiceStrategy::LongestChain);
			block_import_params.storage_changes = Some(storage_changes);

			if let Err(err) = block_import
				.lock()
				.import_block(block_import_params, Default::default())
			{
				error!(
					target: "cumulus-collator",
					"Error importing build block (at {:?}): {:?}",
					b.header().parent_hash(),
					err,
				);
				return Err(InvalidHead);
			}

			let block_data = BlockData(b.encode());
			let head_data = HeadData::<Block> {
				header: b.into_header(),
			};

			let candidate = (
				block_data,
				parachain::HeadData(head_data.encode()),
			);

			trace!(target: "cumulus-collator", "Produced candidate: {:?}", candidate);

			Ok(candidate)
		})
	}
}

/// Implements `BuildParachainContext` to build a collator instance.
pub struct CollatorBuilder<Block: BlockT, PF, BI, Backend, Client> {
	proposer_factory: PF,
	inherent_data_providers: InherentDataProviders,
	block_import: BI,
	para_id: ParaId,
	client: Arc<Client>,
	_marker: PhantomData<(Block, Backend)>,
}

impl<Block: BlockT, PF, BI, Backend, Client>
	CollatorBuilder<Block, PF, BI, Backend, Client>
{
	/// Create a new instance of self.
	pub fn new(
		proposer_factory: PF,
		inherent_data_providers: InherentDataProviders,
		block_import: BI,
		para_id: ParaId,
		client: Arc<Client>,
	) -> Self {
		Self {
			proposer_factory,
			inherent_data_providers,
			block_import,
			para_id,
			client,
			_marker: PhantomData,
		}
	}
}

type TransactionFor<E, Block> =
	<<E as Environment<Block>>::Proposer as Proposer<Block>>::Transaction;

impl<Block: BlockT, PF, BI, Backend, Client> BuildParachainContext
	for CollatorBuilder<Block, PF, BI, Backend, Client>
where
	PF: Environment<Block> + Send + 'static,
	BI: BlockImport<Block, Error = sp_consensus::Error, Transaction = TransactionFor<PF, Block>>
		+ Send
		+ Sync
		+ 'static,
	Backend: sc_client_api::Backend<Block> + 'static,
	Client: Finalizer<Block, Backend> + UsageProvider<Block> + Send + Sync + 'static,
{
	type ParachainContext = Collator<Block, PF, BI>;

	fn build<PClient, Spawner, Extrinsic>(
		self,
		polkadot_client: Arc<PClient>,
		spawner: Spawner,
		network: impl CollatorNetwork + Clone + 'static,
	) -> Result<Self::ParachainContext, ()>
	where
		PClient: ProvideRuntimeApi<PBlock> + Send + Sync + BlockchainEvents<PBlock> + 'static,
		PClient::Api: RuntimeApiCollection<Extrinsic>,
		<PClient::Api as ApiExt<PBlock>>::StateBackend: StateBackend<HashFor<PBlock>>,
		Spawner: Spawn + Clone + Send + Sync + 'static,
		Extrinsic: codec::Codec + Send + Sync + 'static,
	{
		let follow =
			match cumulus_consensus::follow_polkadot(self.para_id, self.client, polkadot_client) {
				Ok(follow) => follow,
				Err(e) => {
					return Err(error!("Could not start following polkadot: {:?}", e));
				}
			};

		spawner
			.spawn_obj(
				Box::new(
					follow.map(|_| ()),
				)
				.into(),
			)
			.map_err(|_| error!("Could not spawn parachain server!"))?;

		Ok(Collator::new(
			self.proposer_factory,
			self.inherent_data_providers,
			network,
			self.block_import,
		))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::time::Duration;

	use polkadot_collator::{collate, SignedStatement};
	use polkadot_primitives::parachain::{HeadData, Id as ParaId};

	use sp_blockchain::Result as ClientResult;
	use sp_inherents::InherentData;
	use sp_keyring::Sr25519Keyring;
	use sp_runtime::{
		generic::BlockId,
		traits::{DigestFor, Header as HeaderT},
	};
	use sp_state_machine::StorageProof;
	use substrate_test_client::{NativeExecutor, WasmExecutionMethod::Interpreted};

	use test_client::{
		DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
	};
	use test_runtime::{Block, Header};

	use futures::{Stream, future};

	#[derive(Debug)]
	struct Error;

	impl From<sp_consensus::Error> for Error {
		fn from(_: sp_consensus::Error) -> Self {
			unimplemented!("Not required in tests")
		}
	}

	struct DummyFactory;

	impl Environment<Block> for DummyFactory {
		type Proposer = DummyProposer;
		type Error = Error;
		type CreateProposer = Pin<Box<
			dyn Future<Output = Result<Self::Proposer, Self::Error>> + Send + Unpin + 'static
		>>;

		fn init(&mut self, _: &Header) -> Self::CreateProposer {
			Box::pin(future::ready(Ok(DummyProposer)))
		}
	}

	struct DummyProposer;

	impl Proposer<Block> for DummyProposer {
		type Error = Error;
		type Proposal = future::Ready<Result<Proposal<Block, Self::Transaction>, Error>>;
		type Transaction = sc_client_api::TransactionFor<test_client::Backend, Block>;

		fn propose(
			&mut self,
			_: InherentData,
			digest: DigestFor<Block>,
			_: Duration,
			_: RecordProof,
		) -> Self::Proposal {
			let header = Header::new(
				1337,
				Default::default(),
				Default::default(),
				Default::default(),
				digest,
			);

			future::ready(Ok(Proposal {
				block: Block::new(header, Vec::new()),
				storage_changes: Default::default(),
				proof: Some(StorageProof::empty()),
			}))
		}
	}

	#[derive(Clone)]
	struct DummyCollatorNetwork;

	impl CollatorNetwork for DummyCollatorNetwork {
		fn checked_statements(&self, _: PHash) -> Pin<Box<dyn Stream<Item = SignedStatement> + Send>> {
			unimplemented!("Not required in tests")
		}
	}

	#[derive(Clone)]
	struct DummyPolkadotClient;

	impl cumulus_consensus::PolkadotClient for DummyPolkadotClient {
		type Error = Error;
		type Finalized = Box<dyn futures::Stream<Item = Vec<u8>> + Send + Unpin>;

		fn finalized_heads(&self, _: ParaId) -> ClientResult<Self::Finalized> {
			unimplemented!("Not required in tests")
		}

		fn parachain_head_at(
			&self,
			_: &BlockId<PBlock>,
			_: ParaId,
		) -> ClientResult<Option<Vec<u8>>> {
			unimplemented!("Not required in tests")
		}
	}

	#[test]
	fn collates_produces_a_block() {
		let id = ParaId::from(100);
		let _ = env_logger::try_init();
		let spawner = futures::executor::ThreadPool::new().unwrap();

		let builder = CollatorBuilder::new(
			DummyFactory,
			InherentDataProviders::default(),
			TestClientBuilder::new().build(),
			id,
			Arc::new(TestClientBuilder::new().build()),
		);
		let context = builder
			.build(
				Arc::new(
					substrate_test_client::TestClientBuilder::<_, _, _, ()>::default()
						.build_with_native_executor::<polkadot_service::polkadot_runtime::RuntimeApi, _>(
							Some(
								NativeExecutor::<polkadot_service::PolkadotExecutor>::new(
									Interpreted,
									None,
									1,
								)
							)
						)
						.0,
				),
				spawner,
				DummyCollatorNetwork,
			)
			.expect("Creates parachain context");

		let header = Header::new(
			0,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		);

		let collation = collate(
			Default::default(),
			id,
			GlobalValidationSchedule {
				block_number: 0,
				max_code_size: 0,
				max_head_data_size: 0,
			},
			LocalValidationData {
				parent_head: HeadData(header.encode()),
				balance: 10,
				code_upgrade_allowed: None,
			},
			context,
			Arc::new(Sr25519Keyring::Alice.pair().into()),
		);

		let collation = futures::executor::block_on(collation).unwrap();

		let block_data = collation.pov.block_data;

		let block = Block::decode(&mut &block_data.0[..]).expect("Is a valid block");

		assert_eq!(1337, *block.header().number());
	}
}
