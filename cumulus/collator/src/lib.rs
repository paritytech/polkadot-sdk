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

use runtime_primitives::traits::Block as BlockT;
use consensus_common::{Environment, Proposer};

use polkadot_collator::{InvalidHead, ParachainContext};
use polkadot_primitives::parachain::{self, BlockData, Message, Id as ParaId, Extrinsic};

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
	inherent_data_providers: inherents::InherentDataProviders,
}

impl<Block: BlockT, PF: Environment<Block>> Collator<Block, PF> {
	/// Create a new instance.
	fn new(
		proposer_factory: Arc<PF>,
		inherent_data_providers: inherent_data::InherentDataProviders
	) -> Self {
		Self {
			proposer_factory,
			inherent_data_providers,
			_phantom: PhantomData,
		}
	}
}

impl<Block, PF> Clone for Collator<Block, PF> {
	fn clone(&self) -> Self {
		Self {
			proposer_factory: self.proposer_factory.clone(),
			inherent_data_providers: self.inherent_data_providers.clone(),
			_phantom: PhantomData,
		}
	}
}

impl<Block, PF> ParachainContext for Collator<Block, PF>
where
	Block: BlockT,
	PF: Environment<Block> + 'static,
	PF::Error: std::fmt::Debug,
{
	type ProduceCandidate = Box<
		dyn Future<Item=(BlockData, parachain::HeadData, Extrinsic), Error=InvalidHead>
	>;

	fn produce_candidate<I: IntoIterator<Item=(ParaId, Message)>>(
		&self,
		last_head: parachain::HeadData,
		_: I,
	) -> Self::ProduceCandidate {
		let factory = self.proposer_factory.clone();
		let inherent_providers = self.inherent_data_providers.clone();

		let res = HeadData::<Block>::decode(&mut &last_head.0[..])
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