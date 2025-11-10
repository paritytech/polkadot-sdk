// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Ensure we're `no_std` when compiling for Wasm.

//! Precompiles for pallet-conviction-voting
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use core::{fmt, marker::PhantomData, num::NonZero};
use frame_support::{
	dispatch::RawOrigin,
	sp_runtime::traits::StaticLookup,
	traits::{Currency, Get, Polling},
	dispatch::DispatchInfo,
};
use pallet_conviction_voting::{AccountVote, Config, Conviction, Tally, Vote, Voting};
use pallet_revive::{
	frame_system,
	precompiles::{
		alloy::{self, sol_types::SolValue},
		AddressMatcher, Error, Ext, Precompile,
	},
	AddressMapper, ExecOrigin as Origin, H160, Weight
};
use tracing::error;

alloy::sol!("src/interfaces/IConvictionVoting.sol");
use IConvictionVoting::IConvictionVotingCalls;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::WeightInfo;

pub type BalanceOf<T> = <<T as pallet_conviction_voting::Config>::Currency as Currency<
	<T as pallet_revive::frame_system::Config>::AccountId,
>>::Balance;

pub type IndexOf<T> = <<T as pallet_conviction_voting::Config>::Polls as Polling<
	Tally<
		<<T as pallet_conviction_voting::Config>::Currency as Currency<
			<T as pallet_revive::frame_system::Config>::AccountId,
		>>::Balance,
		<T as pallet_conviction_voting::Config>::MaxTurnout,
	>,
>>::Index;

pub type ClassOf<T> = <<T as pallet_conviction_voting::Config>::Polls as Polling<
	Tally<
		<<T as pallet_conviction_voting::Config>::Currency as Currency<
			<T as pallet_revive::frame_system::Config>::AccountId,
		>>::Balance,
		<T as pallet_conviction_voting::Config>::MaxTurnout,
	>,
>>::Class;

pub type VotingOf<T> = (
	bool,
	IConvictionVoting::VotingType,
	bool,
	BalanceOf<T>,
	BalanceOf<T>,
	BalanceOf<T>,
	IConvictionVoting::Conviction,
);

const LOG_TARGET: &str = "conviction-voting::precompiles";

fn revert(error: &impl fmt::Debug, message: &str) -> Error {
	error!(target: LOG_TARGET, ?error, "{}", message);
	Error::Revert(message.into())
}

pub struct ConvictionVotingPrecompile<T>(PhantomData<T>);
impl<T> Precompile for ConvictionVotingPrecompile<T>
where
	T: pallet_conviction_voting::Config + pallet_revive::Config,
	BalanceOf<T>: TryFrom<u128> + Into<u128>, // balance as u128
	IndexOf<T>: TryFrom<u32> + TryInto<u32>,  // u32 as ReferendumIndex
	ClassOf<T>: TryFrom<u16> + TryInto<u16>,  // u16 as TrackId
{
	type T = T;
	const MATCHER: AddressMatcher = AddressMatcher::Fixed(NonZero::new(12).unwrap());
	const HAS_CONTRACT_INFO: bool = false;
	type Interface = IConvictionVoting::IConvictionVotingCalls;
	fn call(
		_address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		let origin = env.caller();
		let frame_origin = match origin {
			Origin::Root => RawOrigin::Root.into(),
			Origin::Signed(account_id) => RawOrigin::Signed(account_id.clone()).into(),
		};

		match input {
			IConvictionVotingCalls::voteStandard(_) |
			IConvictionVotingCalls::voteSplit(_) |
			IConvictionVotingCalls::voteSplitAbstain(_) |
			IConvictionVotingCalls::removeVote(_) |
			IConvictionVotingCalls::delegate(_) |
			IConvictionVotingCalls::undelegate(_) |
			IConvictionVotingCalls::unlock(_)
				if env.is_read_only() =>
				Err(Error::Error(pallet_revive::Error::<Self::T>::StateChangeDenied.into())),
			IConvictionVotingCalls::voteStandard(IConvictionVoting::voteStandardCall {
				referendumIndex,
				aye,
				conviction,
				balance,
			}) => {
				let _ = env.charge(<() as WeightInfo>::vote_new()
						.max(<() as WeightInfo>::vote_existing()),
				)?;

				let vote = Vote { aye: *aye, conviction: Self::to_conviction(conviction)? };
				let account_vote =
					AccountVote::Standard { vote, balance: Self::u128_to_balance(balance)? };

				Self::try_vote(frame_origin, referendumIndex, account_vote)
			},
			IConvictionVotingCalls::voteSplit(IConvictionVoting::voteSplitCall {
				referendumIndex,
				ayeAmount,
				nayAmount,
			}) => {
				let _ = env.charge(<() as WeightInfo>::vote_new()
						.max(<() as WeightInfo>::vote_existing()),
				)?;

				let account_vote = AccountVote::Split {
					aye: Self::u128_to_balance(ayeAmount)?,
					nay: Self::u128_to_balance(nayAmount)?,
				};

				Self::try_vote(frame_origin, referendumIndex, account_vote)
			},
			IConvictionVotingCalls::voteSplitAbstain(IConvictionVoting::voteSplitAbstainCall {
				referendumIndex,
				ayeAmount,
				nayAmount,
				abstainAmount,
			}) => {
				let _ = env.charge(<() as WeightInfo>::vote_new()
						.max(<() as WeightInfo>::vote_existing()),
				)?;

				let account_vote = AccountVote::SplitAbstain {
					aye: Self::u128_to_balance(ayeAmount)?,
					nay: Self::u128_to_balance(nayAmount)?,
					abstain: Self::u128_to_balance(abstainAmount)?,
				};

				Self::try_vote(frame_origin, referendumIndex, account_vote)
			},
			IConvictionVotingCalls::delegate(IConvictionVoting::delegateCall {
				trackId,
				to,
				conviction,
				balance,
			}) => {
				let weight_to_charge = <() as WeightInfo>::delegate(<T as Config>::MaxVotes::get());
				let charged_amount = env
					.charge(weight_to_charge)?;

				let target_account_id = T::AddressMapper::to_account_id(&H160::from(to.0 .0));
				let target_source = T::Lookup::unlookup(target_account_id);

				let runtime_conviction = Self::to_conviction(conviction)?;

				let result = pallet_conviction_voting::Pallet::<T>::delegate(
					frame_origin,
					Self::u16_to_track_id(trackId)?,
					target_source,
					runtime_conviction,
					Self::u128_to_balance(balance)?,
				);

				let pre = DispatchInfo {
					call_weight: weight_to_charge,
					extension_weight: Weight::zero(),
					..Default::default()
				};

				// Adjust gas using actual weight or fallback to initially charged weight
				let actual_weight = frame_support::dispatch::extract_actual_weight(&result, &pre);
				env.adjust_gas(charged_amount, actual_weight);

				result.map(|_| Vec::new()).map_err(|error| {
					revert(
							&error,
							"ConvictionVoting: delegation failed"
						)
				})
			},
			IConvictionVotingCalls::undelegate(IConvictionVoting::undelegateCall { trackId }) => {
				let weight_to_charge = <() as WeightInfo>::undelegate(<T as Config>::MaxVotes::get());
				let charged_amount = env.charge(weight_to_charge)?;

				let result = pallet_conviction_voting::Pallet::<T>::undelegate(
					frame_origin,
					Self::u16_to_track_id(trackId)?,
				);

				let pre = DispatchInfo {
					call_weight: weight_to_charge,
					extension_weight: Weight::zero(),
					..Default::default()
				};

				// Adjust gas using actual weight or fallback to initially charged weight
				let actual_weight = frame_support::dispatch::extract_actual_weight(&result, &pre);
				env.adjust_gas(charged_amount, actual_weight);

				result.map(|_| Vec::new()).map_err(|error| {
					revert(
							&error,
							"ConvictionVoting: undelegation failed"
						)
				})
			},
			IConvictionVotingCalls::getVoting(IConvictionVoting::getVotingCall {
				who,
				trackId,
				referendumIndex,
			}) => {
				let _ = env.charge(<() as WeightInfo>::get_voting(
					<T as Config>::MaxVotes::get(),
				))?;

				let who_account_id = T::AddressMapper::to_account_id(&H160::from(who.0 .0));
				let track_id = Self::u16_to_track_id(trackId)?;
				let referendum_index = Self::u32_to_referendum_index(referendumIndex)?;

				Ok(pallet_conviction_voting::VotingFor::<T>::try_get(who_account_id, track_id)
					.ok()
					.and_then(|voting| {
						match voting {
							// If vote is not found, the map returns None and its later defaulted by
							// `map_or_else`
							Voting::Casting(casting) => casting
								.votes
								.iter()
								.find(|(poll_idx, _)| *poll_idx == referendum_index)
								.map(|(_, account_vote)| match account_vote {
									AccountVote::Standard { vote, balance } => (
										true,
										IConvictionVoting::VotingType::Standard,
										vote.aye,
										Self::balance_to_u128(balance) * (vote.aye as u128),
										Self::balance_to_u128(balance) * (!vote.aye as u128),
										0u128,
										Self::from_conviction(vote.conviction),
									),
									AccountVote::Split { aye, nay } => (
										true,
										IConvictionVoting::VotingType::Split,
										false,
										Self::balance_to_u128(aye),
										Self::balance_to_u128(nay),
										0u128,
										IConvictionVoting::Conviction::None,
									),
									AccountVote::SplitAbstain { aye, nay, abstain } => (
										true,
										IConvictionVoting::VotingType::SplitAbstain,
										false,
										Self::balance_to_u128(aye),
										Self::balance_to_u128(nay),
										Self::balance_to_u128(abstain),
										IConvictionVoting::Conviction::None,
									),
								}),
							Voting::Delegating(_) => None, /* propagate the None to return
							                                * default
							                                * voting in `map_or_else */
						}
					})
					.map_or_else(
						|| Self::get_voting_default().abi_encode(),
						|res| res.abi_encode(),
					))
			},
			_ => todo!(),
		}
	}
}

impl<T> ConvictionVotingPrecompile<T>
where
	T: crate::Config + pallet_revive::Config,
	BalanceOf<T>: TryFrom<u128> + Into<u128>, //balance as u128
	IndexOf<T>: TryFrom<u32> + TryInto<u32>,  // u32 as ReferendumIndex
	ClassOf<T>: TryFrom<u16> + TryInto<u16>,  // u16 as TrackId
{
	fn try_vote(
		origin: <T as frame_system::Config>::RuntimeOrigin,
		referendum_index: &u32,
		account_vote: AccountVote<BalanceOf<T>>,
	) -> Result<Vec<u8>, Error> {
		pallet_conviction_voting::Pallet::<T>::vote(
			origin,
			Self::u32_to_referendum_index(referendum_index)?,
			account_vote,
		)
		.map(|_| Vec::new())
		.map_err(|error| revert(&error, "ConvictionVoting: vote failed"))
	}

	fn get_voting_default(
	) -> (bool, IConvictionVoting::VotingType, bool, u128, u128, u128, IConvictionVoting::Conviction)
	{
		(
			false,
			IConvictionVoting::VotingType::Standard,
			false,
			0,
			0,
			0,
			IConvictionVoting::Conviction::None,
		)
	}

	fn u128_to_balance(balance: &u128) -> Result<BalanceOf<T>, Error> {
		(*balance)
			.try_into()
			.map_err(|_| Error::Revert("ConvictionVoting: balance is too large".into()))
	}

	fn balance_to_u128(balance: &BalanceOf<T>) -> u128 {
		(*balance).into()
	}

	fn u16_to_track_id(track_id: &u16) -> Result<ClassOf<T>, Error> {
		(*track_id)
			.try_into()
			.map_err(|_| Error::Revert("ConvictionVoting: trackId is too large".into()))
	}

	fn u32_to_referendum_index(referendum_index: &u32) -> Result<IndexOf<T>, Error> {
		(*referendum_index)
			.try_into()
			.map_err(|_| Error::Revert("ConvictionVoting: referendumIndex is too large".into()))
	}

	fn to_conviction(conviction: &IConvictionVoting::Conviction) -> Result<Conviction, Error> {
		match *conviction {
			IConvictionVoting::Conviction::None => Ok(Conviction::None),
			IConvictionVoting::Conviction::Locked1x => Ok(Conviction::Locked1x),
			IConvictionVoting::Conviction::Locked2x => Ok(Conviction::Locked2x),
			IConvictionVoting::Conviction::Locked3x => Ok(Conviction::Locked3x),
			IConvictionVoting::Conviction::Locked4x => Ok(Conviction::Locked4x),
			IConvictionVoting::Conviction::Locked5x => Ok(Conviction::Locked5x),
			IConvictionVoting::Conviction::Locked6x => Ok(Conviction::Locked6x),
			IConvictionVoting::Conviction::__Invalid =>
				Err(Error::Revert("ConvictionVoting: Invalid conviction value".into())),
		}
	}

	fn from_conviction(conviction: Conviction) -> IConvictionVoting::Conviction {
		match conviction {
			Conviction::None => IConvictionVoting::Conviction::None,
			Conviction::Locked1x => IConvictionVoting::Conviction::Locked1x,
			Conviction::Locked2x => IConvictionVoting::Conviction::Locked2x,
			Conviction::Locked3x => IConvictionVoting::Conviction::Locked3x,
			Conviction::Locked4x => IConvictionVoting::Conviction::Locked4x,
			Conviction::Locked5x => IConvictionVoting::Conviction::Locked5x,
			Conviction::Locked6x => IConvictionVoting::Conviction::Locked6x,
		}
	}
}
