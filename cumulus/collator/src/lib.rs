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

use cumulus_runtime::ParachainBlockData;

use sp_consensus::{
	BlockImport, BlockImportParams, BlockOrigin, Environment, Error as ConsensusError,
	ForkChoiceStrategy, Proposal, Proposer, RecordProof,
};
use sp_inherents::InherentDataProviders;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use sc_cli;

use polkadot_collator::{
	BuildParachainContext, InvalidHead, Network as CollatorNetwork, ParachainContext,
	PolkadotClient,
};
use polkadot_primitives::{
	parachain::{
		self, BlockData, CollatorPair, Id as ParaId, Message, OutgoingMessages,
		Status as ParachainStatus,
	},
	Block as PBlock, Hash as PHash,
};

use codec::{Decode, Encode};

use log::{error, trace};

use futures::{task::Spawn, Future, future};

use std::{
	fmt::Debug, marker::PhantomData, sync::Arc, time::Duration, pin::Pin, collections::HashMap,
};

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
		collator_network: Arc<dyn CollatorNetwork>,
		block_import: BI,
	) -> Self {
		Self {
			proposer_factory: Arc::new(Mutex::new(proposer_factory)),
			inherent_data_providers,
			_phantom: PhantomData,
			collator_network,
			block_import: Arc::new(Mutex::new(block_import)),
		}
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
		dyn Future<Output=Result<(BlockData, parachain::HeadData, OutgoingMessages), InvalidHead>>
		+ Send,
	>>;

	fn produce_candidate<I: IntoIterator<Item=(ParaId, Message)>>(
		&mut self,
		_relay_chain_parent: PHash,
		status: ParachainStatus,
		_: I,
	) -> Self::ProduceCandidate {
		let factory = self.proposer_factory.clone();
		let inherent_providers = self.inherent_data_providers.clone();
		let block_import = self.block_import.clone();

		trace!(target: "cumulus-collator", "Producing candidate");

		let last_head = match HeadData::<Block>::decode(&mut &status.head_data.0[..]) {
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

			let inherent_data = inherent_providers
				.create_inherent_data()
				.map_err(|e| {
					error!(
						target: "cumulus-collator",
						"Failed to create inherent data: {:?}",
						e,
					);
					InvalidHead
				})?;

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
				header,
				extrinsics,
				proof.iter_nodes().collect(),
				parent_state_root,
			);

			let block_import_params = BlockImportParams {
				origin: BlockOrigin::Own,
				header: b.header().clone(),
				justification: None,
				post_digests: vec![],
				body: Some(b.extrinsics().to_vec()),
				finalized: false,
				intermediates: HashMap::new(),
				auxiliary: vec![], // block-weight is written in block import.
				// TODO: block-import handles fork choice and this shouldn't even have the
				// option to specify one.
				// https://github.com/paritytech/substrate/issues/3623
				fork_choice: Some(ForkChoiceStrategy::LongestChain),
				allow_missing_state: false,
				import_existing: false,
				storage_changes: Some(storage_changes),
			};

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
			let messages = OutgoingMessages {
				outgoing_messages: Vec::new(),
			};

			let candidate = (
				block_data,
				parachain::HeadData(head_data.encode()),
				messages,
			);

			trace!(target: "cumulus-collator", "Produced candidate: {:?}", candidate);

			Ok(candidate)
		})
	}
}

/// Implements `BuildParachainContext` to build a collator instance.
struct CollatorBuilder<Block, SP> {
	setup_parachain: SP,
	_marker: PhantomData<Block>,
}

impl<Block, SP> CollatorBuilder<Block, SP> {
	/// Create a new instance of self.
	fn new(setup_parachain: SP) -> Self {
		Self {
			setup_parachain,
			_marker: PhantomData,
		}
	}
}

impl<Block: BlockT, SP: SetupParachain<Block>> BuildParachainContext for CollatorBuilder<Block, SP>
where
	<SP::ProposerFactory as Environment<Block>>::Proposer: Send,
{
	type ParachainContext = Collator<Block, SP::ProposerFactory, SP::BlockImport>;

	fn build<B, E, R, Spawner, Extrinsic>(
		self,
		client: Arc<PolkadotClient<B, E, R>>,
		spawner: Spawner,
		network: Arc<dyn CollatorNetwork>,
	) -> Result<Self::ParachainContext, ()>
	where
		PolkadotClient<B, E, R>: sp_api::ProvideRuntimeApi<PBlock>,
		<PolkadotClient<B, E, R> as sp_api::ProvideRuntimeApi<PBlock>>::Api:
			polkadot_service::RuntimeApiCollection<Extrinsic>,
		E: sc_client::CallExecutor<PBlock> + Clone + Send + Sync + 'static,
		Spawner: Spawn + Clone + Send + Sync + 'static,
		Extrinsic: codec::Codec + Send + Sync + 'static,
		<<PolkadotClient<B, E, R> as sp_api::ProvideRuntimeApi<PBlock>>::Api as sp_api::ApiExt<
			PBlock,
		>>::StateBackend: sp_api::StateBackend<sp_core::Blake2Hasher>,
		R: Send + Sync + 'static,
		B: sc_client_api::Backend<PBlock> + 'static,
		// Rust bug: https://github.com/rust-lang/rust/issues/24159
		B::State: sp_api::StateBackend<sp_core::Blake2Hasher>,
	{
		let (proposer_factory, block_import, inherent_data_providers) = self
			.setup_parachain
			.setup_parachain(client, spawner)
			.map_err(|e| error!("Error setting up the parachain: {}", e))?;

		Ok(Collator::new(
			proposer_factory,
			inherent_data_providers,
			network,
			block_import,
		))
	}
}

/// Something that can setup a parachain.
pub trait SetupParachain<Block: BlockT>: Send {
	/// The proposer factory of the parachain to build blocks.
	type ProposerFactory: Environment<Block> + Send + 'static;
	/// The block import for importing the blocks build by the collator.
	type BlockImport: BlockImport<
			Block,
			Error = ConsensusError,
			Transaction = <<Self::ProposerFactory as Environment<Block>>::Proposer as Proposer<
				Block,
			>>::Transaction,
		> + Send
		+ Sync
		+ 'static;

	/// Setup the parachain.
	fn setup_parachain<P, SP>(
		self,
		polkadot_client: P,
		spawner: SP,
	) -> Result<
		(
			Self::ProposerFactory,
			Self::BlockImport,
			InherentDataProviders,
		),
		String,
	>
	where
		P: cumulus_consensus::PolkadotClient,
		SP: Spawn + Clone + Send + Sync + 'static;
}

/// Run a collator with the given proposer factory.
pub fn run_collator<Block, SP>(
	setup_parachain: SP,
	para_id: ParaId,
	key: Arc<CollatorPair>,
	configuration: polkadot_collator::Configuration,
) -> Result<(), sc_cli::error::Error>
where
	Block: BlockT,
	SP: SetupParachain<Block> + Send + 'static,
	<<SP as SetupParachain<Block>>::ProposerFactory as Environment<Block>>::Proposer: Send,
{
	let builder = CollatorBuilder::new(setup_parachain);
	let exit = future::pending(); // TODO to delete
	polkadot_collator::run_collator(builder, para_id, exit, key, configuration)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::time::Duration;

	use polkadot_collator::{collate, CollatorId, PeerId, RelayChainContext, SignedStatement};
	use polkadot_primitives::parachain::{ConsolidatedIngress, FeeSchedule, HeadData};

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
		Client, DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
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

	struct DummyCollatorNetwork;

	impl CollatorNetwork for DummyCollatorNetwork {
		fn collator_id_to_peer_id(
			&self,
			_: CollatorId,
		) -> Box<dyn Future<Output = Option<PeerId>> + Send> {
			unimplemented!("Not required in tests")
		}

		fn checked_statements(&self, _: PHash) -> Box<dyn Stream<Item = SignedStatement>> {
			unimplemented!("Not required in tests")
		}
	}

	struct DummyRelayChainContext;

	impl RelayChainContext for DummyRelayChainContext {
		type Error = Error;
		type FutureEgress = future::Ready<Result<ConsolidatedIngress, Error>>;

		fn unrouted_egress(&self, _id: ParaId) -> Self::FutureEgress {
			future::ready(Ok(ConsolidatedIngress(Vec::new())))
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

	struct DummySetup;

	impl SetupParachain<Block> for DummySetup {
		type ProposerFactory = DummyFactory;
		type BlockImport = Client;

		fn setup_parachain<P, SP>(
			self,
			_: P,
			_: SP,
		) -> Result<
			(
				Self::ProposerFactory,
				Self::BlockImport,
				InherentDataProviders,
			),
			String,
		> {
			Ok((
				DummyFactory,
				TestClientBuilder::new().build(),
				InherentDataProviders::default(),
			))
		}
	}

	#[test]
	fn collates_produces_a_block() {
		let _ = env_logger::try_init();
		let spawner = futures::executor::ThreadPool::new().unwrap();

		let builder = CollatorBuilder::new(DummySetup);
		let context = builder
			.build::<_, _, polkadot_service::polkadot_runtime::RuntimeApi, _, _>(
				Arc::new(
					substrate_test_client::TestClientBuilder::<_, _, _, ()>::default()
						.build_with_native_executor(Some(NativeExecutor::<
							polkadot_service::PolkadotExecutor,
						>::new(Interpreted, None)))
						.0,
				),
				spawner,
				Arc::new(DummyCollatorNetwork),
			)
			.expect("Creates parachain context");

		let id = ParaId::from(100);
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
			ParachainStatus {
				head_data: HeadData(header.encode()),
				balance: 10,
				fee_schedule: FeeSchedule {
					base: 0,
					per_byte: 1,
				},
			},
			DummyRelayChainContext,
			context,
			Arc::new(Sr25519Keyring::Alice.pair().into()),
		);

		let collation = futures::executor::block_on(collation).unwrap().0;

		let block_data = collation.pov.block_data;

		let block = Block::decode(&mut &block_data.0[..]).expect("Is a valid block");

		assert_eq!(1337, *block.header().number());
	}
}
