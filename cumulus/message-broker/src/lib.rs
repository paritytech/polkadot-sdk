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

//! Cumulus message broker pallet.
//!
//! This pallet provides support for handling downward, upward messages and
//! XMCP messages.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Encode, Decode};
use cumulus_primitives::{
	inherents::{DownwardMessagesType, DOWNWARD_MESSAGES_IDENTIFIER},
	well_known_keys,
	xcmp::{RawXCMPMessage, XCMPMessageHandler, XCMPMessageSender},
	DownwardMessage, DownwardMessageHandler, GenericUpwardMessage, ParaId, UpwardMessageOrigin,
	UpwardMessageSender,
};
use cumulus_upward_message::XCMPMessage;
use frame_support::{
	decl_event, decl_module, storage,
	traits::Get,
	weights::{DispatchClass, Weight},
};
use frame_system::ensure_none;
use sp_inherents::{InherentData, InherentIdentifier, MakeFatalError, ProvideInherent};
use sp_runtime::traits::Hash;
use sp_std::vec::Vec;

/// Configuration trait of this pallet.
pub trait Trait: frame_system::Trait {
	/// Event type used by the runtime.
	type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;

	/// The downward message handlers that will be informed when a message is received.
	type DownwardMessageHandlers: DownwardMessageHandler;

	/// The upward message type used by the Parachain runtime.
	type UpwardMessage: codec::Codec + XCMPMessage;

	/// The XCMP message handlers that will be informed when a XCMP message is received.
	type XCMPMessageHandlers: XCMPMessageHandler<Self::XCMPMessage>;

	/// The XCMP message type used by the Parachain runtime.
	type XCMPMessage: codec::Codec;

	/// The Id of the parachain.
	type ParachainId: Get<ParaId>;
}

decl_event! {
	pub enum Event<T> where Hash = <T as frame_system::Trait>::Hash {
		/// An upward message was sent to the relay chain.
		///
		/// The hash corresponds to the hash of the encoded upward message.
		UpwardMessageSent(Hash),
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin, system = frame_system {
		/// Executes the given downward messages by calling the message handlers.
		///
		/// The origin of this call needs to be `None` as this is an inherent.
		#[weight = (10, DispatchClass::Mandatory)]
		fn execute_downward_messages(origin, messages: Vec<DownwardMessage>) {
			ensure_none(origin)?;

			//TODO: max messages should not be hardcoded. It should be determined based on the
			// weight used by the handlers.
			let max_messages = 10;
			messages.iter().take(max_messages).for_each(|msg| {
				match msg {
					DownwardMessage::XCMPMessage(msg) => {
						if let Ok(msg) = RawXCMPMessage::decode(&mut &msg[..]) {
							if let Ok(xcmp_msg) = T::XCMPMessage::decode(&mut &msg.data[..]) {
								T::XCMPMessageHandlers::handle_xcmp_message(msg.from, &xcmp_msg);
							}
						}
					},
					msg => T::DownwardMessageHandlers::handle_downward_message(msg),
				}
			});

			let processed = sp_std::cmp::min(messages.len(), max_messages) as u32;
			storage::unhashed::put(well_known_keys::PROCESSED_DOWNWARD_MESSAGES, &processed);
		}

		fn on_initialize() -> Weight {
			storage::unhashed::kill(well_known_keys::UPWARD_MESSAGES);

			T::DbWeight::get().writes(1)
		}

		fn deposit_event() = default;
	}
}

impl<T: Trait> UpwardMessageSender<T::UpwardMessage> for Module<T> {
	fn send_upward_message(msg: &T::UpwardMessage, origin: UpwardMessageOrigin) -> Result<(), ()> {
		//TODO: check fee schedule
		let data = msg.encode();
		let data_hash = T::Hashing::hash(&data);

		let msg = GenericUpwardMessage { origin, data };
		sp_io::storage::append(well_known_keys::UPWARD_MESSAGES, msg.encode());

		Self::deposit_event(RawEvent::UpwardMessageSent(data_hash));

		Ok(())
	}
}

impl<T: Trait> XCMPMessageSender<T::XCMPMessage> for Module<T> {
	fn send_xcmp_message(dest: ParaId, msg: &T::XCMPMessage) -> Result<(), ()> {
		let message = RawXCMPMessage {
			from: T::ParachainId::get(),
			data: msg.encode(),
		};

		Self::send_upward_message(
			&T::UpwardMessage::send_message(dest, message.encode()),
			UpwardMessageOrigin::Parachain,
		)
	}
}

impl<T: Trait> ProvideInherent for Module<T> {
	type Call = Call<T>;
	type Error = MakeFatalError<()>;
	const INHERENT_IDENTIFIER: InherentIdentifier = DOWNWARD_MESSAGES_IDENTIFIER;

	fn create_inherent(data: &InherentData) -> Option<Self::Call> {
		data.get_data::<DownwardMessagesType>(&DOWNWARD_MESSAGES_IDENTIFIER)
			.expect("Downward messages inherent data failed to decode")
			.map(|msgs| Call::execute_downward_messages(msgs))
	}
}
