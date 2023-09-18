// Copyright Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! To be removed file that sends messages to the Polkadot Bulletin chain.
//!
//! Right now we miss the Kawabunga chain, so let's emulate it by sending
//! messages to the Polkadot Bulletin chain. 

use crate::{
	bridge_bulletin_config::WITH_POLKADOT_BULLETIN_LANE,
	BridgePolkadotBulletinMessages,
};

use bp_messages::source_chain::MessagesBridge;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> frame_support::weights::Weight {
			// see `encoded_test_xcm_message_to_bulletin_chain` test in the Bulletin
			// chain runtime
			let encoded_xcm_message = hex_literal::hex!("030109002a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a030406020700c817a804017d100000042a");

			// send message to the Bulletin Chain
			let artifacts = BridgePolkadotBulletinMessages::send_message(
				WITH_POLKADOT_BULLETIN_LANE,
				encoded_xcm_message.to_vec(),
			).expect("Something wrong with test config");
			log::trace!(
				target: "runtime::bridge-messsages-generator",
				"Sent message {} to Bulletin Chain",
				artifacts.nonce,
			);

			// don't bother with weights, because we only use this pallet in tests
			Weight::zero()
		}
	}
}
