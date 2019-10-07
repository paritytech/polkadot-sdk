// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

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

use sr_primitives::traits::Block as BlockT;
use consensus_common::{Environment, Proposer};
use inherents::InherentDataProviders;

use polkadot_collator::{
	InvalidHead, ParachainContext, BuildParachainContext, Network as CollatorNetwork, VersionInfo,
};
use polkadot_primitives::{
	Hash,
	parachain::{
		self, BlockData, Message, Id as ParaId, OutgoingMessages, Status as ParachainStatus,
		CollatorPair,
	}
};

use codec::{Decode, Encode};

use log::error;

use futures03::TryFutureExt;
use futures::{Future, future::IntoFuture};

use std::{sync::Arc, marker::PhantomData, time::Duration};

use parking_lot::Mutex;

/// The head data of the parachain, stored in the relay chain.
#[derive(Decode, Encode, Debug)]
struct HeadData<Block: BlockT> {
	header: Block::Header,
}

/// The implementation of the Cumulus `Collator`.
pub struct Collator<Block, PF> {
	proposer_factory: Arc<Mutex<PF>>,
	_phantom: PhantomData<Block>,
	inherent_data_providers: InherentDataProviders,
	collator_network: Arc<dyn CollatorNetwork>,
}

impl<Block: BlockT, PF: Environment<Block>> Collator<Block, PF> {
	/// Create a new instance.
	fn new(
		proposer_factory: PF,
		inherent_data_providers: InherentDataProviders,
		collator_network: Arc<dyn CollatorNetwork>,
	) -> Self {
		Self {
			proposer_factory: Arc::new(Mutex::new(proposer_factory)),
			inherent_data_providers,
			_phantom: PhantomData,
			collator_network,
		}
	}
}

impl<Block, PF> Clone for Collator<Block, PF> {
	fn clone(&self) -> Self {
		Self {
			proposer_factory: self.proposer_factory.clone(),
			inherent_data_providers: self.inherent_data_providers.clone(),
			_phantom: PhantomData,
			collator_network: self.collator_network.clone(),
		}
	}
}

impl<Block, PF> ParachainContext for Collator<Block, PF> where
	Block: BlockT,
	PF: Environment<Block> + 'static + Send + Sync,
	PF::Error: std::fmt::Debug,
	PF::Proposer: Send + Sync,
	<PF::Proposer as Proposer<Block>>::Create: Unpin + Send + Sync,
{
	type ProduceCandidate = Box<
		dyn Future<Item=(BlockData, parachain::HeadData, OutgoingMessages), Error=InvalidHead>
			+ Send + Sync
	>;

	fn produce_candidate<I: IntoIterator<Item=(ParaId, Message)>>(
		&self,
		_relay_chain_parent: Hash,
		status: ParachainStatus,
		_: I,
	) -> Self::ProduceCandidate {
		let factory = self.proposer_factory.clone();
		let inherent_providers = self.inherent_data_providers.clone();

		let res = HeadData::<Block>::decode(&mut &status.head_data.0[..])
			.map_err(|_| InvalidHead)
			.into_future()
			.and_then(move |last_head|
				factory.lock()
					.init(&last_head.header)
					.map_err(|e| {
						//TODO: Do we want to return the real error?
						error!("Could not create proposer: {:?}", e);
						InvalidHead
					})
			)
			.and_then(move |proposer|
				inherent_providers.create_inherent_data()
					.map(|id| (proposer, id))
					.map_err(|e| {
						error!("Failed to create inherent data: {:?}", e);
						InvalidHead
					})
			)
			.and_then(|(mut proposer, inherent_data)| {
				proposer.propose(
					inherent_data,
					Default::default(),
					//TODO: Fix this.
					Duration::from_secs(6),
				)
				.map_err(|e| {
					error!("Proposing failed: {:?}", e);
					InvalidHead
				})
				.compat()
			})
			.map(|b| {
				let block_data = BlockData(b.encode());
				let head_data = HeadData::<Block> { header: b.deconstruct().0 };
				let messages = OutgoingMessages { outgoing_messages: Vec::new() };

				(block_data, parachain::HeadData(head_data.encode()), messages)
			});

		Box::new(res)
	}
}

/// Implements `BuildParachainContext` to build a collator instance.
struct CollatorBuilder<Block, PF> {
	inherent_data_providers: InherentDataProviders,
	proposer_factory: PF,
	_phantom: PhantomData<Block>,
}

impl<Block, PF> CollatorBuilder<Block, PF> {
	/// Create a new instance of self.
	fn new(proposer_factory: PF, inherent_data_providers: InherentDataProviders) -> Self {
		Self {
			inherent_data_providers,
			proposer_factory,
			_phantom: Default::default(),
		}
	}
}

impl<Block, PF> BuildParachainContext for CollatorBuilder<Block, PF> where
	Block: BlockT,
	PF: Environment<Block> + 'static + Send + Sync,
	PF::Error: std::fmt::Debug,
	PF::Proposer: Send + Sync,
	<PF::Proposer as Proposer<Block>>::Create: Unpin + Send + Sync,
{
	type ParachainContext = Collator<Block, PF>;

	fn build(self, network: Arc<dyn CollatorNetwork>) -> Result<Self::ParachainContext, ()> {
		Ok(Collator::new(self.proposer_factory, self.inherent_data_providers, network))
	}
}

/// Run a collator with the given proposer factory.
pub fn run_collator<Block, PF, E, I>(
	proposer_factory: PF,
	inherent_data_providers: InherentDataProviders,
	para_id: ParaId,
	exit: E,
	key: Arc<CollatorPair>,
	version: VersionInfo,
) -> Result<(), cli::error::Error>
where
	Block: BlockT,
	PF: Environment<Block> + 'static + Send + Sync,
	PF::Error: std::fmt::Debug,
	PF::Proposer: Send + Sync,
	<PF::Proposer as Proposer<Block>>::Create: Unpin + Send + Sync,
	E: IntoFuture<Item=(), Error=()>,
	E::Future: Send + Clone + Sync + 'static,
{
	let builder = CollatorBuilder::new(proposer_factory, inherent_data_providers);
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
		type Create = future::Ready<Result<Block, Error>>;

		fn propose(
			&mut self,
			_: InherentData,
			digest : DigestFor<Block>,
			_: Duration,
		) -> Self::Create {
			let header = Header::new(
				1337,
				Default::default(),
				Default::default(),
				Default::default(),
				digest,
			);

			future::ready(Ok(Block::new(header, Vec::new())))
		}
	}

	struct DummyCollatorNetwork;

	impl CollatorNetwork for DummyCollatorNetwork {
		fn collator_id_to_peer_id(&self, _: CollatorId) ->
			Box<dyn Future<Item=Option<PeerId>, Error=()> + Send>
		{
			unimplemented!("Not required in tests")
		}

		fn checked_statements(&self, _: Hash) ->
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

	#[test]
	fn collates_produces_a_block() {
		let builder = CollatorBuilder::new(DummyFactory, InherentDataProviders::new());
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
}
