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

use runtime_primitives::{traits::Block as BlockT, ed25519};
use consensus_common::{Environment, Proposer};
use inherents::InherentDataProviders;

use polkadot_collator::{
	InvalidHead, ParachainContext, BuildParachainContext, Network as CollatorNetwork, VersionInfo,
};
use polkadot_primitives::{
	Hash, parachain::{self, BlockData, Message, Id as ParaId, Extrinsic, Status as ParachainStatus}
};

use codec::{Decode, Encode};
use log::error;
use futures::{Future, future::IntoFuture};

use std::{sync::Arc, marker::PhantomData, time::Duration};

/// The head data of the parachain, stored in the relay chain.
#[derive(Decode, Encode, Debug)]
struct HeadData<Block: BlockT> {
	header: Block::Header,
}

/// The implementation of the Cumulus `Collator`.
pub struct Collator<Block, PF> {
	proposer_factory: Arc<PF>,
	_phantom: PhantomData<Block>,
	inherent_data_providers: InherentDataProviders,
	collator_network: Arc<dyn CollatorNetwork>,
}

impl<Block: BlockT, PF: Environment<Block>> Collator<Block, PF> {
	/// Create a new instance.
	fn new(
		proposer_factory: Arc<PF>,
		inherent_data_providers: InherentDataProviders,
		collator_network: Arc<dyn CollatorNetwork>,
	) -> Self {
		Self {
			proposer_factory,
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
	PF: Environment<Block> + 'static,
	PF::Error: std::fmt::Debug,
{
	type ProduceCandidate = Box<
		dyn Future<Item=(BlockData, parachain::HeadData, Extrinsic), Error=InvalidHead>
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
			.ok_or_else(|| InvalidHead).into_future()
			.and_then(move |last_head|
				factory.init(&last_head.header).map_err(|e| {
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
			.and_then(|(proposer, inherent_data)| {
				proposer.propose(
					inherent_data,
					Default::default(),
					//TODO: Fix this.
					Duration::from_secs(6),
				)
				.into_future()
				.map_err(|e| {
					error!("Proposing failed: {:?}", e);
					InvalidHead
				})
			})
			.map(|b| {
				let block_data = BlockData(b.encode());
				let head_data = HeadData::<Block> { header: b.deconstruct().0 };
				let extrinsic = Extrinsic { outgoing_messages: Vec::new() };

				(block_data, parachain::HeadData(head_data.encode()), extrinsic)
			});

		Box::new(res)
	}
}

/// Implements `BuildParachainContext` to build a collator instance.
struct CollatorBuilder<Block, PF> {
	inherent_data_providers: InherentDataProviders,
	proposer_factory: Arc<PF>,
	_phantom: PhantomData<Block>,
}

impl<Block, PF> CollatorBuilder<Block, PF> {
	/// Create a new instance of self.
	fn new(proposer_factory: Arc<PF>, inherent_data_providers: InherentDataProviders) -> Self {
		Self {
			inherent_data_providers,
			proposer_factory,
			_phantom: Default::default(),
		}
	}
}

impl<Block, PF> BuildParachainContext for CollatorBuilder<Block, PF> where
	Block: BlockT,
	PF: Environment<Block> + 'static,
	PF::Error: std::fmt::Debug,
{
	type ParachainContext = Collator<Block, PF>;

	fn build(self, network: Arc<dyn CollatorNetwork>) -> Result<Self::ParachainContext, ()> {
		Ok(Collator::new(self.proposer_factory, self.inherent_data_providers, network))
	}
}

/// Run a collator with the given proposer factory.
pub fn run_collator<Block, PF, E, I>(
	proposer_factory: Arc<PF>,
	inherent_data_providers: InherentDataProviders,
	para_id: ParaId,
	exit: E,
	key: Arc<ed25519::Pair>,
	args: I,
	version: VersionInfo,
) -> Result<(), cli::error::Error>
where
	Block: BlockT,
	PF: Environment<Block> + 'static + Send,
	PF::Error: std::fmt::Debug
{
	let builder = CollatorBuilder::new(proposer_factory, inherent_data_providers);
	polkadot_collator::run_collator(builder, para_id, exit, key, args, version)
}