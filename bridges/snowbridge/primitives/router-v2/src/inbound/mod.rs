// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Converts messages from Ethereum to XCM messages

#[cfg(test)]
mod tests;

use codec::{Decode, Encode};
use core::marker::PhantomData;
use frame_support::{traits::tokens::Balance as BalanceT, weights::Weight, PalletError};
use scale_info::TypeInfo;
use snowbridge_core::TokenId;
use sp_core::{Get, RuntimeDebug, H160, H256};
use sp_io::hashing::blake2_256;
use sp_runtime::{traits::MaybeEquivalence, MultiAddress};
use sp_std::prelude::*;
use xcm::prelude::{Junction::AccountKey20, *};
use xcm_executor::traits::ConvertLocation;

const MINIMUM_DEPOSIT: u128 = 1;

/// Messages from Ethereum are versioned. This is because in future,
/// we may want to evolve the protocol so that the ethereum side sends XCM messages directly.
/// Instead having BridgeHub transcode the messages into XCM.
#[derive(Clone, Encode, Decode, RuntimeDebug)]
pub enum VersionedMessage {
	V2(Message),
}

/// For V2, the ethereum side sends messages which are transcoded into XCM. These messages are
/// self-contained, in that they can be transcoded using only information in the message.
#[derive(Clone, Encode, Decode, RuntimeDebug)]
pub struct Message {
	/// The origin address
	pub origin: H160,
	/// The command originating from the Gateway contract
	pub xcm: Vec<u8>,
}

pub struct GlobalConsensusEthereumConvertsFor<AccountId>(PhantomData<AccountId>);
impl<AccountId> ConvertLocation<AccountId> for GlobalConsensusEthereumConvertsFor<AccountId>
where
	AccountId: From<[u8; 32]> + Clone,
{
	fn convert_location(location: &Location) -> Option<AccountId> {
		match location.unpack() {
			(_, [GlobalConsensus(Ethereum { chain_id })]) =>
				Some(Self::from_chain_id(chain_id).into()),
			_ => None,
		}
	}
}

impl<AccountId> GlobalConsensusEthereumConvertsFor<AccountId> {
	pub fn from_chain_id(chain_id: &u64) -> [u8; 32] {
		(b"ethereum-chain", chain_id).using_encoded(blake2_256)
	}
}
