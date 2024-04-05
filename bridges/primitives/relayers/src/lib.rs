// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Primitives of messages module.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

pub use registration::{Registration, StakeAndSlash};

use bp_messages::LaneId;
use bp_runtime::{ChainId, StorageDoubleMapKeyProvider};
use frame_support::{traits::tokens::Preservation, Blake2_128Concat, Identity};
use scale_info::TypeInfo;
use sp_runtime::{
	codec::{Codec, Decode, Encode, EncodeLike, MaxEncodedLen},
	traits::AccountIdConversion,
	TypeId,
};
use sp_std::{fmt::Debug, marker::PhantomData};

mod registration;

/// The owner of the sovereign account that should pay the rewards.
///
/// Each of the 2 final points connected by a bridge owns a sovereign account at each end of the
/// bridge. So here, at this end of the bridge there can be 2 sovereign accounts that pay rewards.
#[derive(Copy, Clone, Debug, Decode, Encode, Eq, PartialEq, TypeInfo, MaxEncodedLen)]
pub enum RewardsAccountOwner {
	/// The sovereign account of the final chain on this end of the bridge.
	ThisChain,
	/// The sovereign account of the final chain on the other end of the bridge.
	BridgedChain,
}

/// Structure used to identify the account that pays a reward to the relayer.
///
/// A bridge connects 2 bridge ends. Each one is located on a separate relay chain. The bridge ends
/// can be the final destinations of the bridge, or they can be intermediary points
/// (e.g. a bridge hub) used to forward messages between pairs of parachains on the bridged relay
/// chains. A pair of such parachains is connected using a bridge lane. Each of the 2 final
/// destinations of a bridge lane must have a sovereign account at each end of the bridge and each
/// of the sovereign accounts will pay rewards for different operations. So we need multiple
/// parameters to identify the account that pays a reward to the relayer.
#[derive(Copy, Clone, Debug, Decode, Encode, Eq, PartialEq, TypeInfo, MaxEncodedLen)]
pub struct RewardsAccountParams {
	lane_id: LaneId,
	bridged_chain_id: ChainId,
	owner: RewardsAccountOwner,
}

impl RewardsAccountParams {
	/// Create a new instance of `RewardsAccountParams`.
	pub const fn new(
		lane_id: LaneId,
		bridged_chain_id: ChainId,
		owner: RewardsAccountOwner,
	) -> Self {
		Self { lane_id, bridged_chain_id, owner }
	}
}

impl TypeId for RewardsAccountParams {
	const TYPE_ID: [u8; 4] = *b"brap";
}

/// Reward payment procedure.
pub trait PaymentProcedure<Relayer, Reward> {
	/// Error that may be returned by the procedure.
	type Error: Debug;

	/// Pay reward to the relayer from the account with provided params.
	fn pay_reward(
		relayer: &Relayer,
		rewards_account_params: RewardsAccountParams,
		reward: Reward,
	) -> Result<(), Self::Error>;
}

impl<Relayer, Reward> PaymentProcedure<Relayer, Reward> for () {
	type Error = &'static str;

	fn pay_reward(_: &Relayer, _: RewardsAccountParams, _: Reward) -> Result<(), Self::Error> {
		Ok(())
	}
}

/// Reward payment procedure that does `balances::transfer` call from the account, derived from
/// given params.
pub struct PayRewardFromAccount<T, Relayer>(PhantomData<(T, Relayer)>);

impl<T, Relayer> PayRewardFromAccount<T, Relayer>
where
	Relayer: Decode + Encode,
{
	/// Return account that pays rewards based on the provided parameters.
	pub fn rewards_account(params: RewardsAccountParams) -> Relayer {
		params.into_sub_account_truncating(b"rewards-account")
	}
}

impl<T, Relayer> PaymentProcedure<Relayer, T::Balance> for PayRewardFromAccount<T, Relayer>
where
	T: frame_support::traits::fungible::Mutate<Relayer>,
	Relayer: Decode + Encode + Eq,
{
	type Error = sp_runtime::DispatchError;

	fn pay_reward(
		relayer: &Relayer,
		rewards_account_params: RewardsAccountParams,
		reward: T::Balance,
	) -> Result<(), Self::Error> {
		T::transfer(
			&Self::rewards_account(rewards_account_params),
			relayer,
			reward,
			Preservation::Expendable,
		)
		.map(drop)
	}
}

/// Can be use to access the runtime storage key within the `RelayerRewards` map of the relayers
/// pallet.
pub struct RelayerRewardsKeyProvider<AccountId, Reward>(PhantomData<(AccountId, Reward)>);

impl<AccountId, Reward> StorageDoubleMapKeyProvider for RelayerRewardsKeyProvider<AccountId, Reward>
where
	AccountId: Codec + EncodeLike,
	Reward: Codec + EncodeLike,
{
	const MAP_NAME: &'static str = "RelayerRewards";

	type Hasher1 = Blake2_128Concat;
	type Key1 = AccountId;
	type Hasher2 = Identity;
	type Key2 = RewardsAccountParams;
	type Value = Reward;
}

#[cfg(test)]
mod tests {
	use super::*;
	use bp_messages::LaneId;
	use sp_runtime::testing::H256;

	#[test]
	fn different_lanes_are_using_different_accounts() {
		assert_eq!(
			PayRewardFromAccount::<(), H256>::rewards_account(RewardsAccountParams::new(
				LaneId([0, 0, 0, 0]),
				*b"test",
				RewardsAccountOwner::ThisChain
			)),
			hex_literal::hex!("62726170000000007465737400726577617264732d6163636f756e7400000000")
				.into(),
		);

		assert_eq!(
			PayRewardFromAccount::<(), H256>::rewards_account(RewardsAccountParams::new(
				LaneId([0, 0, 0, 1]),
				*b"test",
				RewardsAccountOwner::ThisChain
			)),
			hex_literal::hex!("62726170000000017465737400726577617264732d6163636f756e7400000000")
				.into(),
		);
	}

	#[test]
	fn different_directions_are_using_different_accounts() {
		assert_eq!(
			PayRewardFromAccount::<(), H256>::rewards_account(RewardsAccountParams::new(
				LaneId([0, 0, 0, 0]),
				*b"test",
				RewardsAccountOwner::ThisChain
			)),
			hex_literal::hex!("62726170000000007465737400726577617264732d6163636f756e7400000000")
				.into(),
		);

		assert_eq!(
			PayRewardFromAccount::<(), H256>::rewards_account(RewardsAccountParams::new(
				LaneId([0, 0, 0, 0]),
				*b"test",
				RewardsAccountOwner::BridgedChain
			)),
			hex_literal::hex!("62726170000000007465737401726577617264732d6163636f756e7400000000")
				.into(),
		);
	}
}
