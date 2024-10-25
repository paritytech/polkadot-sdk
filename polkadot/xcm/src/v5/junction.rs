// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Support data structures for `Location`, primarily the `Junction` datatype.

use super::Location;
pub use crate::v4::{BodyId, BodyPart};
use crate::{
	v4::{Junction as OldJunction, NetworkId as OldNetworkId},
	VersionedLocation,
};
use bounded_collections::{BoundedSlice, BoundedVec, ConstU32};
use codec::{self, Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

/// A single item in a path to describe the relative location of a consensus system.
///
/// Each item assumes a pre-existing location as its context and is defined in terms of it.
#[derive(
	Copy,
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Encode,
	Decode,
	Debug,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
)]
pub enum Junction {
	/// An indexed parachain belonging to and operated by the context.
	///
	/// Generally used when the context is a Polkadot Relay-chain.
	Parachain(#[codec(compact)] u32),
	/// A 32-byte identifier for an account of a specific network that is respected as a sovereign
	/// endpoint within the context.
	///
	/// Generally used when the context is a Substrate-based chain.
	AccountId32 { network: Option<NetworkId>, id: [u8; 32] },
	/// An 8-byte index for an account of a specific network that is respected as a sovereign
	/// endpoint within the context.
	///
	/// May be used when the context is a Frame-based chain and includes e.g. an indices pallet.
	AccountIndex64 {
		network: Option<NetworkId>,
		#[codec(compact)]
		index: u64,
	},
	/// A 20-byte identifier for an account of a specific network that is respected as a sovereign
	/// endpoint within the context.
	///
	/// May be used when the context is an Ethereum or Bitcoin chain or smart-contract.
	AccountKey20 { network: Option<NetworkId>, key: [u8; 20] },
	/// An instanced, indexed pallet that forms a constituent part of the context.
	///
	/// Generally used when the context is a Frame-based chain.
	PalletInstance(u8),
	/// A non-descript index within the context location.
	///
	/// Usage will vary widely owing to its generality.
	///
	/// NOTE: Try to avoid using this and instead use a more specific item.
	GeneralIndex(#[codec(compact)] u128),
	/// A nondescript array datum, 32 bytes, acting as a key within the context
	/// location.
	///
	/// Usage will vary widely owing to its generality.
	///
	/// NOTE: Try to avoid using this and instead use a more specific item.
	// Note this is implemented as an array with a length rather than using `BoundedVec` owing to
	// the bound for `Copy`.
	GeneralKey { length: u8, data: [u8; 32] },
	/// The unambiguous child.
	///
	/// Not currently used except as a fallback when deriving context.
	OnlyChild,
	/// A pluralistic body existing within consensus.
	///
	/// Typical to be used to represent a governance origin of a chain, but could in principle be
	/// used to represent things such as multisigs also.
	Plurality { id: BodyId, part: BodyPart },
	/// A global network capable of externalizing its own consensus. This is not generally
	/// meaningful outside of the universal level.
	GlobalConsensus(NetworkId),
}

/// A global identifier of a data structure existing within consensus.
///
/// Maintenance note: Networks with global consensus and which are practically bridgeable within the
/// Polkadot ecosystem are given preference over explicit naming in this enumeration.
#[derive(
	Copy,
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Encode,
	Decode,
	Debug,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
)]
pub enum NetworkId {
	/// Network specified by the first 32 bytes of its genesis block.
	ByGenesis([u8; 32]),
	/// Network defined by the first 32-bytes of the hash and number of some block it contains.
	ByFork { block_number: u64, block_hash: [u8; 32] },
	/// The Polkadot mainnet Relay-chain.
	Polkadot,
	/// The Kusama canary-net Relay-chain.
	Kusama,
	/// The Westend testnet Relay-chain.
	Westend,
	/// The Rococo testnet Relay-chain.
	Rococo,
	/// The Wococo testnet Relay-chain.
	Wococo,
	/// An Ethereum network specified by its chain ID.
	Ethereum {
		/// The EIP-155 chain ID.
		#[codec(compact)]
		chain_id: u64,
	},
	/// The Bitcoin network, including hard-forks supported by Bitcoin Core development team.
	BitcoinCore,
	/// The Bitcoin network, including hard-forks supported by Bitcoin Cash developers.
	BitcoinCash,
	/// The Polkadot Bulletin chain.
	PolkadotBulletin,
}

impl From<OldNetworkId> for Option<NetworkId> {
	fn from(old: OldNetworkId) -> Self {
		Some(NetworkId::from(old))
	}
}

impl From<OldNetworkId> for NetworkId {
	fn from(old: OldNetworkId) -> Self {
		use OldNetworkId::*;
		match old {
			ByGenesis(hash) => Self::ByGenesis(hash),
			ByFork { block_number, block_hash } => Self::ByFork { block_number, block_hash },
			Polkadot => Self::Polkadot,
			Kusama => Self::Kusama,
			Westend => Self::Westend,
			Rococo => Self::Rococo,
			Wococo => Self::Wococo,
			Ethereum { chain_id } => Self::Ethereum { chain_id },
			BitcoinCore => Self::BitcoinCore,
			BitcoinCash => Self::BitcoinCash,
			PolkadotBulletin => Self::PolkadotBulletin,
		}
	}
}

impl From<NetworkId> for Junction {
	fn from(n: NetworkId) -> Self {
		Self::GlobalConsensus(n)
	}
}

impl From<[u8; 32]> for Junction {
	fn from(id: [u8; 32]) -> Self {
		Self::AccountId32 { network: None, id }
	}
}

impl From<BoundedVec<u8, ConstU32<32>>> for Junction {
	fn from(key: BoundedVec<u8, ConstU32<32>>) -> Self {
		key.as_bounded_slice().into()
	}
}

impl<'a> From<BoundedSlice<'a, u8, ConstU32<32>>> for Junction {
	fn from(key: BoundedSlice<'a, u8, ConstU32<32>>) -> Self {
		let mut data = [0u8; 32];
		data[..key.len()].copy_from_slice(&key[..]);
		Self::GeneralKey { length: key.len() as u8, data }
	}
}

impl<'a> TryFrom<&'a Junction> for BoundedSlice<'a, u8, ConstU32<32>> {
	type Error = ();
	fn try_from(key: &'a Junction) -> Result<Self, ()> {
		match key {
			Junction::GeneralKey { length, data } =>
				BoundedSlice::try_from(&data[..data.len().min(*length as usize)]).map_err(|_| ()),
			_ => Err(()),
		}
	}
}

impl From<[u8; 20]> for Junction {
	fn from(key: [u8; 20]) -> Self {
		Self::AccountKey20 { network: None, key }
	}
}

impl From<u64> for Junction {
	fn from(index: u64) -> Self {
		Self::AccountIndex64 { network: None, index }
	}
}

impl From<u128> for Junction {
	fn from(id: u128) -> Self {
		Self::GeneralIndex(id)
	}
}

impl TryFrom<OldJunction> for Junction {
	type Error = ();
	fn try_from(value: OldJunction) -> Result<Self, ()> {
		use OldJunction::*;
		Ok(match value {
			Parachain(id) => Self::Parachain(id),
			AccountId32 { network: maybe_network, id } =>
				Self::AccountId32 { network: maybe_network.map(|network| network.into()), id },
			AccountIndex64 { network: maybe_network, index } =>
				Self::AccountIndex64 { network: maybe_network.map(|network| network.into()), index },
			AccountKey20 { network: maybe_network, key } =>
				Self::AccountKey20 { network: maybe_network.map(|network| network.into()), key },
			PalletInstance(index) => Self::PalletInstance(index),
			GeneralIndex(id) => Self::GeneralIndex(id),
			GeneralKey { length, data } => Self::GeneralKey { length, data },
			OnlyChild => Self::OnlyChild,
			Plurality { id, part } => Self::Plurality { id, part },
			GlobalConsensus(network) => Self::GlobalConsensus(network.into()),
		})
	}
}

impl Junction {
	/// Convert `self` into a `Location` containing 0 parents.
	///
	/// Similar to `Into::into`, except that this method can be used in a const evaluation context.
	pub fn into_location(self) -> Location {
		Location::new(0, [self])
	}

	/// Convert `self` into a `Location` containing `n` parents.
	///
	/// Similar to `Self::into_location`, with the added ability to specify the number of parent
	/// junctions.
	pub fn into_exterior(self, n: u8) -> Location {
		Location::new(n, [self])
	}

	/// Convert `self` into a `VersionedLocation` containing 0 parents.
	///
	/// Similar to `Into::into`, except that this method can be used in a const evaluation context.
	pub fn into_versioned(self) -> VersionedLocation {
		self.into_location().into_versioned()
	}

	/// Remove the `NetworkId` value.
	pub fn remove_network_id(&mut self) {
		use Junction::*;
		match self {
			AccountId32 { ref mut network, .. } |
			AccountIndex64 { ref mut network, .. } |
			AccountKey20 { ref mut network, .. } => *network = None,
			_ => {},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::vec;

	#[test]
	fn junction_round_trip_works() {
		let j = Junction::GeneralKey { length: 32, data: [1u8; 32] };
		let k = Junction::try_from(OldJunction::try_from(j).unwrap()).unwrap();
		assert_eq!(j, k);

		let j = OldJunction::GeneralKey { length: 32, data: [1u8; 32] };
		let k = OldJunction::try_from(Junction::try_from(j).unwrap()).unwrap();
		assert_eq!(j, k);

		let j = Junction::from(BoundedVec::try_from(vec![1u8, 2, 3, 4]).unwrap());
		let k = Junction::try_from(OldJunction::try_from(j).unwrap()).unwrap();
		assert_eq!(j, k);
		let s: BoundedSlice<_, _> = (&k).try_into().unwrap();
		assert_eq!(s, &[1u8, 2, 3, 4][..]);

		let j = OldJunction::GeneralKey { length: 32, data: [1u8; 32] };
		let k = OldJunction::try_from(Junction::try_from(j).unwrap()).unwrap();
		assert_eq!(j, k);
	}
}
