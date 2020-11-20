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
	weights::{DispatchClass, Weight},
	traits::Get,
	StorageValue,
};
use frame_system::ensure_none;
use sp_inherents::{InherentData, InherentIdentifier, MakeFatalError, ProvideInherent};
use sp_std::{prelude::*, cmp};

use cumulus_primitives::{
	inherents::{DownwardMessagesType, DOWNWARD_MESSAGES_IDENTIFIER},
	well_known_keys, DownwardMessageHandler, InboundDownwardMessage, UpwardMessage,
};

/// Configuration trait of the message broker pallet.
pub trait Trait: frame_system::Trait {
	/// The downward message handlers that will be informed when a message is received.
	type DownwardMessageHandlers: DownwardMessageHandler;
}

decl_storage! {
	trait Store for Module<T: Trait> as MessageBroker {
		PendingUpwardMessages: Vec<UpwardMessage>;
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		/// An entrypoint for an inherent to deposit downward messages into the runtime. It accepts
		/// and processes the list of downward messages.
		#[weight = (10, DispatchClass::Mandatory)]
		fn receive_downward_messages(origin, messages: Vec<InboundDownwardMessage>) {
			ensure_none(origin)?;

			let messages_len = messages.len() as u32;
			for message in messages {
				T::DownwardMessageHandlers::handle_downward_message(message);
			}

			// Store the processed_downward_messages here so that it's will be accessible from
			// PVF's `validate_block` wrapper and collation pipeline.
			storage::unhashed::put(
				well_known_keys::PROCESSED_DOWNWARD_MESSAGES,
				&messages_len,
			);
		}

		fn on_initialize() -> Weight {
			// Reads and writes performed by `on_finalize`.
			T::DbWeight::get().reads_writes(1, 2)
		}

		fn on_finalize() {
			// TODO: this should be not a constant, but sourced from the relay-chain configuration.
			const UMP_MSG_NUM_PER_CANDIDATE: usize = 5;

			<Self as Store>::PendingUpwardMessages::mutate(|up| {
				let num = cmp::min(UMP_MSG_NUM_PER_CANDIDATE, up.len());
				storage::unhashed::put(
					well_known_keys::UPWARD_MESSAGES,
					&up[0..num],
				);
				*up = up.split_off(num);
			});
		}
	}
}

/// An error that can be raised upon sending an upward message.
pub enum SendUpErr {
	/// The message sent is too big.
	TooBig,
}

impl<T: Trait> Module<T> {
	pub fn send_upward_message(message: UpwardMessage) -> Result<(), SendUpErr> {
		// TODO: check the message against the limit. The limit should be sourced from the
		// relay-chain configuration.
		<Self as Store>::PendingUpwardMessages::append(message);
		Ok(())
	}
}

impl<T: Trait> ProvideInherent for Module<T> {
	type Call = Call<T>;
	type Error = MakeFatalError<()>;
	const INHERENT_IDENTIFIER: InherentIdentifier = DOWNWARD_MESSAGES_IDENTIFIER;

	fn create_inherent(data: &InherentData) -> Option<Self::Call> {
		data.get_data::<DownwardMessagesType>(&DOWNWARD_MESSAGES_IDENTIFIER)
			.expect("Downward messages inherent data failed to decode")
			.map(|msgs| Call::receive_downward_messages(msgs))
	}
}
