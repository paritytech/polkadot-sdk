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
// along with Polkadot. If not, see <http://www.gnu.org/licenses/>.

//! Types copied from the SDK nomination pools pallet.
//! TODO delete once we integrated the changes into the SDK.

use crate::*;
use pallet_nomination_pools::{BalanceOf, PoolId, TotalUnbondingPools};
use sp_staking::EraIndex;

// From https://github.com/paritytech/polkadot-sdk/blob/bf20a9ee18f7215210bbbabf79e955c8c35b3360/substrate/frame/nomination-pools/src/lib.rs#L1301
#[derive(
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	CloneNoBound,
	PartialEqNoBound,
	EqNoBound,
	RuntimeDebugNoBound,
)]
#[codec(mel_bound(T: Config))]
#[scale_info(skip_type_params(T))]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub struct RewardPool<T: pallet_nomination_pools::Config> {
	/// The last recorded value of the reward counter.
	///
	/// This is updated ONLY when the points in the bonded pool change, which means `join`,
	/// `bond_extra` and `unbond`, all of which is done through `update_recorded`.
	pub last_recorded_reward_counter: T::RewardCounter,
	/// The last recorded total payouts of the reward pool.
	///
	/// Payouts is essentially income of the pool.
	///
	/// Update criteria is same as that of `last_recorded_reward_counter`.
	pub last_recorded_total_payouts: BalanceOf<T>,
	/// Total amount that this pool has paid out so far to the members.
	pub total_rewards_claimed: BalanceOf<T>,
	/// The amount of commission pending to be claimed.
	pub total_commission_pending: BalanceOf<T>,
	/// The amount of commission that has been claimed.
	pub total_commission_claimed: BalanceOf<T>,
}

// From https://github.com/paritytech/polkadot-sdk/blob/bf20a9ee18f7215210bbbabf79e955c8c35b3360/substrate/frame/nomination-pools/src/lib.rs#L1503
#[derive(
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	RuntimeDebugNoBound,
	CloneNoBound,
	PartialEqNoBound,
	EqNoBound,
)]
#[codec(mel_bound(T: Config))]
#[scale_info(skip_type_params(T))]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub struct SubPools<T: pallet_nomination_pools::Config> {
	/// A general, era agnostic pool of funds that have fully unbonded. The pools
	/// of `Self::with_era` will lazily be merged into into this pool if they are
	/// older then `current_era - TotalUnbondingPools`.
	pub no_era: UnbondPool<T>,
	/// Map of era in which a pool becomes unbonded in => unbond pools.
	pub with_era: BoundedBTreeMap<EraIndex, UnbondPool<T>, TotalUnbondingPools<T>>,
}

// From https://github.com/paritytech/polkadot-sdk/blob/bf20a9ee18f7215210bbbabf79e955c8c35b3360/substrate/frame/nomination-pools/src/lib.rs#L1461
#[derive(
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	RuntimeDebugNoBound,
	CloneNoBound,
	PartialEqNoBound,
	EqNoBound,
)]
#[codec(mel_bound(T: Config))]
#[scale_info(skip_type_params(T))]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub struct UnbondPool<T: pallet_nomination_pools::Config> {
	/// The points in this pool.
	pub points: BalanceOf<T>,
	/// The funds in the pool.
	pub balance: BalanceOf<T>,
}

// From https://github.com/paritytech/polkadot-sdk/blob/bf20a9ee18f7215210bbbabf79e955c8c35b3360/substrate/frame/nomination-pools/src/lib.rs#L1718-L1719
#[frame_support::storage_alias(pallet_name)]
pub type SubPoolsStorage<T: pallet_nomination_pools::Config> =
	CountedStorageMap<pallet_nomination_pools::Pallet<T>, Twox64Concat, PoolId, SubPools<T>>;

// From https://github.com/paritytech/polkadot-sdk/blob/bf20a9ee18f7215210bbbabf79e955c8c35b3360/substrate/frame/nomination-pools/src/lib.rs#L1713-L1714
#[frame_support::storage_alias(pallet_name)]
pub type RewardPools<T: pallet_nomination_pools::Config> =
	CountedStorageMap<pallet_nomination_pools::Pallet<T>, Twox64Concat, PoolId, RewardPool<T>>;
