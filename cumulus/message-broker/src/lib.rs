// Copyright 2020 Parity Technologies (UK) Ltd.
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

//! This pallet deals with the low-level details of parachain message passing.
//!
//! Specifically, this pallet serves as a glue layer between cumulus collation pipeline and the
//! runtime that hosts this pallet.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
	decl_module, decl_storage, storage,
	traits::Get,
	weights::{DispatchClass, Weight},
	StorageValue,
};
use frame_system::{ensure_none, ensure_root};
use sp_inherents::{InherentData, InherentIdentifier, MakeFatalError, ProvideInherent};
use sp_std::{cmp, prelude::*};

use cumulus_primitives::{
	inherents::{MessageIngestionType, MESSAGE_INGESTION_IDENTIFIER},
	well_known_keys, DownwardMessageHandler, HrmpMessageHandler, OutboundHrmpMessage, ParaId,
	UpwardMessage,
};

// TODO: these should be not a constant, but sourced from the relay-chain configuration.
const UMP_MSG_NUM_PER_CANDIDATE: usize = 5;
const HRMP_MSG_NUM_PER_CANDIDATE: usize = 5;

/// Configuration trait of the message broker pallet.
pub trait Config: frame_system::Config {
	/// The downward message handlers that will be informed when a message is received.
	type DownwardMessageHandlers: DownwardMessageHandler;
	/// The HRMP message handlers that will be informed when a message is received.
	type HrmpMessageHandlers: HrmpMessageHandler;
}

decl_storage! {
	trait Store for Module<T: Config> as MessageBroker {
		PendingUpwardMessages: Vec<UpwardMessage>;

		/// Essentially `OutboundHrmpMessage`s grouped by the recipients.
		OutboundHrmpMessages: map hasher(twox_64_concat) ParaId => Vec<Vec<u8>>;
		/// HRMP channels with the given recipients are awaiting to be processed. If a `ParaId` is
		/// present in this vector then `OutboundHrmpMessages` for it should be not empty.
		NonEmptyHrmpChannels: Vec<ParaId>;
	}
}

decl_module! {
	pub struct Module<T: Config> for enum Call where origin: T::Origin {
		/// An entrypoint for an inherent to deposit downward messages into the runtime. It accepts
		/// and processes the list of downward messages and inbound HRMP messages.
		#[weight = (10, DispatchClass::Mandatory)]
		fn ingest_inbound_messages(origin, messages: MessageIngestionType) {
			ensure_none(origin)?;

			let MessageIngestionType {
				downward_messages,
				horizontal_messages,
			} = messages;

			let dm_count = downward_messages.len() as u32;
			for downward_message in downward_messages {
				T::DownwardMessageHandlers::handle_downward_message(downward_message);
			}

			// Store the processed_downward_messages here so that it's will be accessible from
			// PVF's `validate_block` wrapper and collation pipeline.
			storage::unhashed::put(
				well_known_keys::PROCESSED_DOWNWARD_MESSAGES,
				&dm_count,
			);

			let mut hrmp_watermark = None;
			for (sender, channel_contents) in horizontal_messages {
				for horizontal_message in channel_contents {
					if hrmp_watermark
						.map(|w| w < horizontal_message.sent_at)
						.unwrap_or(true)
					{
						hrmp_watermark = Some(horizontal_message.sent_at);
					}

					T::HrmpMessageHandlers::handle_hrmp_message(sender, horizontal_message);
				}
			}

			// If we processed at least one message, then advance watermark to that location.
			if let Some(hrmp_watermark) = hrmp_watermark {
				storage::unhashed::put(
					well_known_keys::HRMP_WATERMARK,
					&hrmp_watermark,
				);
			}
		}

		#[weight = (1_000, DispatchClass::Operational)]
		fn sudo_send_upward_message(origin, message: UpwardMessage) {
			ensure_root(origin)?;
			let _ = Self::send_upward_message(message);
		}

		#[weight = (1_000, DispatchClass::Operational)]
		fn sudo_send_hrmp_message(origin, message: OutboundHrmpMessage) {
			ensure_root(origin)?;
			let _ = Self::send_hrmp_message(message);
		}

		fn on_initialize() -> Weight {
			let mut weight = T::DbWeight::get().writes(3);
			storage::unhashed::kill(well_known_keys::HRMP_WATERMARK);
			storage::unhashed::kill(well_known_keys::UPWARD_MESSAGES);
			storage::unhashed::kill(well_known_keys::HRMP_OUTBOUND_MESSAGES);

			// Reads and writes performed by `on_finalize`. This may actually turn out to be lower,
			// but we should err on the safe side.
			weight += T::DbWeight::get().reads_writes(
				2 + HRMP_MSG_NUM_PER_CANDIDATE as u64,
				4 + HRMP_MSG_NUM_PER_CANDIDATE as u64,
			);

			weight
		}

		fn on_finalize() {
			<Self as Store>::PendingUpwardMessages::mutate(|up| {
				let num = cmp::min(UMP_MSG_NUM_PER_CANDIDATE, up.len());
				storage::unhashed::put(
					well_known_keys::UPWARD_MESSAGES,
					&up[0..num],
				);
				*up = up.split_off(num);
			});

			// Sending HRMP messages is a little bit more involved. On top of the number of messages
			// per block limit, there is also a constraint that it's possible to send only a single
			// message to a given recipient per candidate.
			let mut non_empty_hrmp_channels = NonEmptyHrmpChannels::get();
			let outbound_hrmp_num = cmp::min(HRMP_MSG_NUM_PER_CANDIDATE, non_empty_hrmp_channels.len());
			let mut outbound_hrmp_messages = Vec::with_capacity(outbound_hrmp_num);
			let mut prune_empty = Vec::with_capacity(outbound_hrmp_num);

			for &recipient in non_empty_hrmp_channels.iter().take(outbound_hrmp_num) {
				let (message_payload, became_empty) =
					<Self as Store>::OutboundHrmpMessages::mutate(&recipient, |v| {
						// this panics if `v` is empty. However, we are iterating only once over non-empty
						// channels, therefore it cannot panic.
						let first = v.remove(0);
						let became_empty = v.is_empty();
						(first, became_empty)
					});

				outbound_hrmp_messages.push(OutboundHrmpMessage {
					recipient,
					data: message_payload,
				});
				if became_empty {
					prune_empty.push(recipient);
				}
			}

			// Prune hrmp channels that became empty. Additionally, because it may so happen that we
			// only gave attention to some channels in `non_empty_hrmp_channels` it's important to
			// change the order. Otherwise, the next `on_finalize` we will again give attention
			// only to those channels that happen to be in the beginning, until they are emptied.
			// This leads to "starvation" of the channels near to the end.
			//
			// To mitigate this we shift all processed elements towards the end of the vector using
			// `rotate_left`. To get intution how it works see the examples in its rustdoc.
			non_empty_hrmp_channels.retain(|x| !prune_empty.contains(x));
			non_empty_hrmp_channels.rotate_left(outbound_hrmp_num - prune_empty.len());

			<Self as Store>::NonEmptyHrmpChannels::put(non_empty_hrmp_channels);
			storage::unhashed::put(
				well_known_keys::HRMP_OUTBOUND_MESSAGES,
				&outbound_hrmp_messages,
			);
		}
	}
}

/// An error that can be raised upon sending an upward message.
pub enum SendUpErr {
	/// The message sent is too big.
	TooBig,
}

/// An error that can be raised upon sending a horizontal message.
pub enum SendHorizonalErr {
	/// The message sent is too big.
	TooBig,
	/// There is no channel to the specified destination.
	NoChannel,
}

impl<T: Config> Module<T> {
	pub fn send_upward_message(message: UpwardMessage) -> Result<(), SendUpErr> {
		// TODO: check the message against the limit. The limit should be sourced from the
		// relay-chain configuration.
		<Self as Store>::PendingUpwardMessages::append(message);
		Ok(())
	}

	pub fn send_hrmp_message(message: OutboundHrmpMessage) -> Result<(), SendHorizonalErr> {
		// TODO:
		// (a) check against the size limit sourced from the relay-chain configuration
		// (b) check if the channel to the recipient is actually opened.

		let OutboundHrmpMessage { recipient, data } = message;
		<Self as Store>::OutboundHrmpMessages::append(&recipient, data);

		<Self as Store>::NonEmptyHrmpChannels::mutate(|v| {
			if !v.contains(&recipient) {
				v.push(recipient);
			}
		});

		Ok(())
	}
}

impl<T: Config> ProvideInherent for Module<T> {
	type Call = Call<T>;
	type Error = MakeFatalError<()>;
	const INHERENT_IDENTIFIER: InherentIdentifier = MESSAGE_INGESTION_IDENTIFIER;

	fn create_inherent(data: &InherentData) -> Option<Self::Call> {
		data.get_data::<MessageIngestionType>(&MESSAGE_INGESTION_IDENTIFIER)
			.expect("Downward messages inherent data failed to decode")
			.map(|msgs| Call::ingest_inbound_messages(msgs))
	}
}
