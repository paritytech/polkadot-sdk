// Copyright Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use frame_support::{parameter_types, weights::Weight};
use xcm::latest::prelude::*;
use xcm_simulator::{
	AggregateMessageOrigin, ProcessMessage, ProcessMessageError, UmpQueueId, WeightMeter,
};

use crate::relay_chain::{RuntimeCall, XcmConfig};

parameter_types! {
	/// Amount of weight that can be spent per block to service messages.
	pub MessageQueueServiceWeight: Weight = Weight::from_parts(1_000_000_000, 1_000_000);
	pub const MessageQueueHeapSize: u32 = 65_536;
	pub const MessageQueueMaxStale: u32 = 16;
}

/// Message processor to handle any messages that were enqueued into the `MessageQueue` pallet.
pub struct MessageProcessor;
impl ProcessMessage for MessageProcessor {
	type Origin = AggregateMessageOrigin;

	fn process_message(
		message: &[u8],
		origin: Self::Origin,
		meter: &mut WeightMeter,
		id: &mut [u8; 32],
	) -> Result<bool, ProcessMessageError> {
		let para = match origin {
			AggregateMessageOrigin::Ump(UmpQueueId::Para(para)) => para,
		};
		xcm_builder::ProcessXcmMessage::<
			Junction,
			xcm_executor::XcmExecutor<XcmConfig>,
			RuntimeCall,
		>::process_message(message, Junction::Parachain(para.into()), meter, id)
	}
}
