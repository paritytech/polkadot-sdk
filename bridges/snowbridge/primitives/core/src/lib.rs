// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! # Core
//!
//! Common traits and types
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod tests;

pub mod inbound;
pub mod operating_mode;
pub mod outbound;
pub mod pricing;
pub mod ringbuffer;

pub use polkadot_parachain_primitives::primitives::{
	Id as ParaId, IsSystem, Sibling as SiblingParaId,
};
pub use ringbuffer::{RingBufferMap, RingBufferMapImpl};
pub use sp_core::U256;

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::traits::Contains;
use hex_literal::hex;
use scale_info::TypeInfo;
use sp_core::H256;
use sp_io::hashing::keccak_256;
use sp_runtime::{traits::AccountIdConversion, RuntimeDebug};
use sp_std::prelude::*;
use xcm::prelude::{Junction::Parachain, Location};
use xcm_builder::{DescribeAllTerminal, DescribeFamily, DescribeLocation, HashedDescription};

/// The ID of an agent contract
pub type AgentId = H256;
pub use operating_mode::BasicOperatingMode;

pub use pricing::{PricingParameters, Rewards};

pub fn sibling_sovereign_account<T>(para_id: ParaId) -> T::AccountId
where
	T: frame_system::Config,
{
	SiblingParaId::from(para_id).into_account_truncating()
}

pub struct AllowSiblingsOnly;
impl Contains<Location> for AllowSiblingsOnly {
	fn contains(location: &Location) -> bool {
		matches!(location.unpack(), (1, [Parachain(_)]))
	}
}

pub fn gwei(x: u128) -> U256 {
	U256::from(1_000_000_000u128).saturating_mul(x.into())
}

pub fn meth(x: u128) -> U256 {
	U256::from(1_000_000_000_000_000u128).saturating_mul(x.into())
}

pub fn eth(x: u128) -> U256 {
	U256::from(1_000_000_000_000_000_000u128).saturating_mul(x.into())
}

pub const ROC: u128 = 1_000_000_000_000;

/// Identifier for a message channel
#[derive(
	Clone, Copy, Encode, Decode, PartialEq, Eq, Default, RuntimeDebug, MaxEncodedLen, TypeInfo,
)]
pub struct ChannelId([u8; 32]);

/// Deterministically derive a ChannelId for a sibling parachain
/// Generator: keccak256("para" + big_endian_bytes(para_id))
///
/// The equivalent generator on the Solidity side is in
/// contracts/src/Types.sol:into().
fn derive_channel_id_for_sibling(para_id: ParaId) -> ChannelId {
	let para_id: u32 = para_id.into();
	let para_id_bytes: [u8; 4] = para_id.to_be_bytes();
	let prefix: [u8; 4] = *b"para";
	let preimage: Vec<u8> = prefix.into_iter().chain(para_id_bytes).collect();
	keccak_256(&preimage).into()
}

impl ChannelId {
	pub const fn new(id: [u8; 32]) -> Self {
		ChannelId(id)
	}
}

impl From<ParaId> for ChannelId {
	fn from(value: ParaId) -> Self {
		derive_channel_id_for_sibling(value)
	}
}

impl From<[u8; 32]> for ChannelId {
	fn from(value: [u8; 32]) -> Self {
		ChannelId(value)
	}
}

impl From<ChannelId> for [u8; 32] {
	fn from(value: ChannelId) -> Self {
		value.0
	}
}

impl<'a> From<&'a [u8; 32]> for ChannelId {
	fn from(value: &'a [u8; 32]) -> Self {
		ChannelId(*value)
	}
}

impl From<H256> for ChannelId {
	fn from(value: H256) -> Self {
		ChannelId(value.into())
	}
}

impl AsRef<[u8]> for ChannelId {
	fn as_ref(&self) -> &[u8] {
		&self.0
	}
}

#[derive(Clone, Encode, Decode, RuntimeDebug, MaxEncodedLen, TypeInfo)]
pub struct Channel {
	/// ID of the agent contract deployed on Ethereum
	pub agent_id: AgentId,
	/// ID of the parachain who will receive or send messages using this channel
	pub para_id: ParaId,
}

pub trait StaticLookup {
	/// Type to lookup from.
	type Source;
	/// Type to lookup into.
	type Target;
	/// Attempt a lookup.
	fn lookup(s: Self::Source) -> Option<Self::Target>;
}

/// Channel for high-priority governance commands
pub const PRIMARY_GOVERNANCE_CHANNEL: ChannelId =
	ChannelId::new(hex!("0000000000000000000000000000000000000000000000000000000000000001"));

/// Channel for lower-priority governance commands
pub const SECONDARY_GOVERNANCE_CHANNEL: ChannelId =
	ChannelId::new(hex!("0000000000000000000000000000000000000000000000000000000000000002"));

pub struct DescribeHere;
impl DescribeLocation for DescribeHere {
	fn describe_location(l: &Location) -> Option<Vec<u8>> {
		match l.unpack() {
			(0, []) => Some(Vec::<u8>::new().encode()),
			_ => None,
		}
	}
}

/// Creates an AgentId from a Location. An AgentId is a unique mapping to a Agent contract on
/// Ethereum which acts as the sovereign account for the Location.
pub type AgentIdOf = HashedDescription<H256, (DescribeHere, DescribeFamily<DescribeAllTerminal>)>;
