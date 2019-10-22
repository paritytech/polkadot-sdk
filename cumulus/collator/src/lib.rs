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

use sr_primitives::traits::{Block as BlockT, Header as HeaderT};
use consensus_common::{
	BlockImport, Environment, Proposer, ForkChoiceStrategy, BlockImportParams, BlockOrigin,
	Error as ConsensusError,
};
use inherents::InherentDataProviders;
use substrate_primitives::Blake2Hasher;

use polkadot_collator::{
	InvalidHead, ParachainContext, BuildParachainContext, Network as CollatorNetwork, VersionInfo,
	TaskExecutor, PolkadotClient,
};
use polkadot_primitives::{
	Hash as PHash, Block as PBlock,
	parachain::{
		self, BlockData, Message, Id as ParaId, OutgoingMessages, Status as ParachainStatus,
		CollatorPair,
	}
};

use codec::{Decode, Encode};

use log::{error, trace};

use futures03::{TryFutureExt, future};
use futures::{Future, future::IntoFuture};

use std::{sync::Arc, marker::PhantomData, time::Duration, fmt::Debug};

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
		BI: BlockImport<Block, Error=ConsensusError> + Send + Sync + 'static,
{
	type ProduceCandidate = Box<
		dyn Future<Item=(BlockData, parachain::HeadData, OutgoingMessages), Error=InvalidHead>
			+ Send
	>;

	fn produce_candidate<I: IntoIterator<Item=(ParaId, Message)>>(
		&mut self,
		_relay_chain_parent: PHash,
		status: ParachainStatus,
		_: I,
	) -> Self::ProduceCandidate {
		trace!(target: "cumulus-collator", "Producing candidate");

		let factory = self.proposer_factory.clone();
		let inherent_providers = self.inherent_data_providers.clone();
		let block_import = self.block_import.clone();

		let res = HeadData::<Block>::decode(&mut &status.head_data.0[..])
			.map_err(|e| {
				error!(target: "cumulus-collator", "Could not decode the head data: {:?}", e);
				InvalidHead
			})
			.into_future()
			.and_then(move |last_head| {
				let parent_state_root = *last_head.header.state_root();

				factory.lock()
					.init(&last_head.header)
					.map_err(|e| {
						error!(
							target: "cumulus-collator",
							"Could not create proposer: {:?}",
							e,
						);
						InvalidHead
					})
					.and_then(|mut proposer| {
						let inherent_data = inherent_providers.create_inherent_data()
							.map_err(|e| {
								error!(
								target: "cumulus-collator",
									"Failed to create inherent data: {:?}",
									e,
								);
								InvalidHead
							})?;

						let future = proposer.propose(
							inherent_data,
							Default::default(),
							//TODO: Fix this.
							Duration::from_secs(6),
							true,
						)
						.map_err(|e| {
							error!(
								target: "cumulus-collator",
								"Proposing failed: {:?}",
								e,
							);
							InvalidHead
						})
						.and_then(move |(block, proof)| {
							let res = match proof {
								Some(proof) => {
									let (header, extrinsics) = block.deconstruct();

									// Create the parachain block data for the validators.
									Ok(
										ParachainBlockData::<Block>::new(
											header,
											extrinsics,
											proof,
											parent_state_root,
										)
									)
								}
								None => {
									error!(
										target: "cumulus-collator",
										"Proposer did not return the requested proof.",
									);
									Err(InvalidHead)
								}
							};

							future::ready(res)
						})
						.compat();

						Ok(future)
					})
			})
			.flatten()
			.and_then(move |b| {
				let block_import_params = BlockImportParams {
					origin: BlockOrigin::Own,
					header: b.header().clone(),
					justification: None,
					post_digests: vec![],
					body: Some(b.extrinsics().to_vec()),
					finalized: false,
					auxiliary: vec![], // block-weight is written in block import.
					// TODO: block-import handles fork choice and this shouldn't even have the
					// option to specify one.
					// https://github.com/paritytech/substrate/issues/3623
					fork_choice: ForkChoiceStrategy::LongestChain,
				};

				if let Err(err) = block_import.lock().import_block(
					block_import_params,
					Default::default(),
				) {
					error!(
						target: "cumulus-collator",
						"Error importing build block (at {:?}): {:?}",
						b.header().parent_hash(),
						err,
					);
					Err(InvalidHead)
				} else {
					Ok(b)
				}
			})
			.map(|b| {
				let block_data = BlockData(b.encode());
				let head_data = HeadData::<Block> { header: b.into_header() };
				let messages = OutgoingMessages { outgoing_messages: Vec::new() };

				(block_data, parachain::HeadData(head_data.encode()), messages)
			})
			.then(|r| {
				trace!(target: "cumulus-collator", "Produced candidate: {:?}", r);
				r
			});

		Box::new(res)
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

impl<Block: BlockT, SP: SetupParachain<Block>> BuildParachainContext for CollatorBuilder<Block, SP> {
	type ParachainContext = Collator<Block, SP::ProposerFactory, SP::BlockImport>;

	fn build<B, E>(
		self,
		client: Arc<PolkadotClient<B, E>>,
		task_executor: TaskExecutor,
		network: Arc<dyn CollatorNetwork>,
	) -> Result<Self::ParachainContext, ()>
		where
			B: substrate_client::backend::Backend<PBlock, Blake2Hasher> + 'static,
			E: substrate_client::CallExecutor<PBlock, Blake2Hasher> + Clone + Send + Sync + 'static
	{
		let (proposer_factory, block_import, inherent_data_providers) = self.setup_parachain
			.setup_parachain(client, task_executor)
			.map_err(|e| error!("Error setting up the parachain: {}", e))?;

		Ok(Collator::new(proposer_factory, inherent_data_providers, network, block_import))
	}
}

/// Something that can setup a parachain.
pub trait SetupParachain<Block: BlockT>: Send {
	/// The proposer factory of the parachain to build blocks.
	type ProposerFactory: Environment<Block> + Send + 'static;
	/// The block import for importing the blocks build by the collator.
	type BlockImport: BlockImport<Block, Error=ConsensusError> + Send + Sync + 'static;

	/// Setup the parachain.
	fn setup_parachain<P: cumulus_consensus::PolkadotClient>(
		self,
		polkadot_client: P,
		task_executor: TaskExecutor,
	) -> Result<(Self::ProposerFactory, Self::BlockImport, InherentDataProviders), String>;
}

/// Run a collator with the given proposer factory.
pub fn run_collator<Block, SP, E>(
	setup_parachain: SP,
	para_id: ParaId,
	exit: E,
	key: Arc<CollatorPair>,
	version: VersionInfo,
) -> Result<(), cli::error::Error>
where
	Block: BlockT,
	SP: SetupParachain<Block> + Send + 'static,
	E: IntoFuture<Item=(), Error=()>,
	E::Future: Send + Clone + Sync + 'static,
{
	let builder = CollatorBuilder::new(setup_parachain);
	polkadot_collator::run_collator(builder, para_id, exit, key, version)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::time::Duration;

	use polkadot_collator::{collate, RelayChainContext, PeerId, CollatorId, SignedStatement};
	use polkadot_primitives::parachain::{ConsolidatedIngress, HeadData, FeeSchedule};

	use keyring::Sr25519Keyring;
	use sr_primitives::traits::{DigestFor, Header as HeaderT};
	use inherents::InherentData;

	use test_runtime::{Block, Header};

	use futures03::future;
	use futures::Stream;

	#[derive(Debug)]
	struct Error;

	impl From<consensus_common::Error> for Error {
		fn from(_: consensus_common::Error) -> Self {
			unimplemented!("Not required in tests")
		}
	}

	struct DummyFactory;

	impl Environment<Block> for DummyFactory {
		type Proposer = DummyProposer;
		type Error = Error;

		fn init(&mut self, _: &Header) -> Result<Self::Proposer, Self::Error> {
			Ok(DummyProposer)
		}
	}

	struct DummyProposer;

	impl Proposer<Block> for DummyProposer {
		type Error = Error;
		type Proposal = future::Ready<Result<(Block, Option<Vec<Vec<u8>>>), Error>>;

		fn propose(
			&mut self,
			_: InherentData,
			digest : DigestFor<Block>,
			_: Duration,
			_: bool,
		) -> Self::Proposal {
			let header = Header::new(
				1337,
				Default::default(),
				Default::default(),
				Default::default(),
				digest,
			);

			future::ready(Ok((Block::new(header, Vec::new()), None)))
		}
	}

	struct DummyCollatorNetwork;

	impl CollatorNetwork for DummyCollatorNetwork {
		fn collator_id_to_peer_id(&self, _: CollatorId) ->
			Box<dyn Future<Item=Option<PeerId>, Error=()> + Send>
		{
			unimplemented!("Not required in tests")
		}

		fn checked_statements(&self, _: PHash) ->
			Box<dyn Stream<Item=SignedStatement, Error=()>>
		{
			unimplemented!("Not required in tests")
		}
	}

	struct DummyRelayChainContext;

	impl RelayChainContext for DummyRelayChainContext {
		type Error = Error;
		type FutureEgress = Result<ConsolidatedIngress, Self::Error>;

		fn unrouted_egress(&self, _id: ParaId) -> Self::FutureEgress {
			Ok(ConsolidatedIngress(Vec::new()))
		}
	}
/*
	#[test]
	fn collates_produces_a_block() {
		let builder = CollatorBuilder::new(DummyFactory);
		let context = builder.build(Arc::new(DummyCollatorNetwork)).expect("Creates parachain context");

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
		).wait().unwrap().0;

		let block_data = collation.pov.block_data;

		let block = Block::decode(&mut &block_data.0[..]).expect("Is a valid block");

		assert_eq!(1337, *block.header().number());
	}
	*/
}
