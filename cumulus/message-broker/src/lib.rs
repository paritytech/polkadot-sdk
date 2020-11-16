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

use sp_std::prelude::*;
use frame_system::ensure_none;
use frame_support::{decl_module, storage, weights::DispatchClass};
use sp_inherents::{InherentData, InherentIdentifier, MakeFatalError, ProvideInherent};

use cumulus_primitives::{
	inherents::{DownwardMessagesType, DOWNWARD_MESSAGES_IDENTIFIER},
	well_known_keys,
	DownwardMessageHandler, InboundDownwardMessage,
};

/// Configuration trait of the message broker pallet.
pub trait Trait: frame_system::Trait {
	/// The downward message handlers that will be informed when a message is received.
	type DownwardMessageHandlers: DownwardMessageHandler;
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
