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
//! This pallet provides support for handling downward and upward messages.

#![cfg_attr(not(feature = "std"), no_std)]

use cumulus_primitives::{
	inherents::{DownwardMessagesType, DOWNWARD_MESSAGES_IDENTIFIER},
	well_known_keys, DownwardMessageHandler, UpwardMessageSender,
};
use frame_support::{
	decl_module, storage,
	weights::{SimpleDispatchInfo, WeighData, Weight},
};
use frame_system::ensure_none;
use sp_inherents::{InherentData, InherentIdentifier, MakeFatalError, ProvideInherent};

/// Configuration trait of this pallet.
pub trait Trait: frame_system::Trait {
	/// The downward message handlers that will be informed when a message is received.
	type DownwardMessageHandlers: DownwardMessageHandler;
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		/// Executes the given downward messages by calling the message handlers.
		///
		/// The origin of this call needs to be `None` as this is an inherent.
		#[weight = SimpleDispatchInfo::FixedMandatory(10)]
		fn execute_downward_messages(origin, messages: Vec<()>) {
			ensure_none(origin)?;
			messages.iter().for_each(T::DownwardMessageHandlers::handle_downward_message);
		}

		fn on_initialize() -> Weight {
			storage::unhashed::kill(well_known_keys::UPWARD_MESSAGES);

			SimpleDispatchInfo::default().weigh_data(())
		}
	}
}

impl<T: Trait> UpwardMessageSender for Module<T> {
	fn send_upward_message(_: &()) -> Result<(), ()> {
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
			.map(|msgs| Call::execute_downward_messages(msgs))
	}
}
