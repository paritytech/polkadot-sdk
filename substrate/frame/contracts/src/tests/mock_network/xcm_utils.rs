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

use codec::Encode;
use core::{borrow::Borrow, marker::PhantomData};
use sp_core::blake2_256;
use xcm::prelude::*;
use xcm_executor::traits::{ConvertLocation };

/// Prefix for generating alias account for accounts coming
/// from chains that use 32 byte long representations.
pub const FOREIGN_CHAIN_PREFIX_PARA_32: [u8; 37] = *b"ForeignChainAliasAccountPrefix_Para32";

/// Prefix for generating alias account for accounts coming
/// from the relay chain using 32 byte long representations.
pub const FOREIGN_CHAIN_PREFIX_RELAY: [u8; 36] = *b"ForeignChainAliasAccountPrefix_Relay";

pub struct ForeignChainAliasAccount<AccountId>(PhantomData<AccountId>);
impl<AccountId: From<[u8; 32]> + Clone> ConvertLocation<AccountId>
	for ForeignChainAliasAccount<AccountId>
{
	fn convert_location(location: &MultiLocation) -> Option<AccountId> {
		let entropy = match location.borrow() {
			// Used on the relay chain for sending paras that use 32 byte accounts
			MultiLocation {
				parents: 0,
				interior: X2(Parachain(para_id), AccountId32 { id, .. }),
			} => ForeignChainAliasAccount::<AccountId>::from_para_32(para_id, id, 0),

			// Used on para-chain for sending paras that use 32 byte accounts
			MultiLocation {
				parents: 1,
				interior: X2(Parachain(para_id), AccountId32 { id, .. }),
			} => ForeignChainAliasAccount::<AccountId>::from_para_32(para_id, id, 1),

			// Used on para-chain for sending from the relay chain
			MultiLocation { parents: 1, interior: X1(AccountId32 { id, .. }) } =>
				ForeignChainAliasAccount::<AccountId>::from_relay_32(id, 1),

			// No other conversions provided
			_ => return None,
		};

		Some(entropy.into())
	}
}

impl<AccountId> ForeignChainAliasAccount<AccountId> {
	fn from_para_32(para_id: &u32, id: &[u8; 32], parents: u8) -> [u8; 32] {
		(FOREIGN_CHAIN_PREFIX_PARA_32, para_id, id, parents).using_encoded(blake2_256)
	}

	fn from_relay_32(id: &[u8; 32], parents: u8) -> [u8; 32] {
		(FOREIGN_CHAIN_PREFIX_RELAY, id, parents).using_encoded(blake2_256)
	}
}
