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
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use core::{fmt, marker::PhantomData, num::NonZero};
use frame_support::{
	dispatch::RawOrigin,
	sp_runtime::traits::StaticLookup,
	traits::{Currency, Get, Polling},
};
use pallet_conviction_voting::{AccountVote, Config, Conviction, Tally, Vote, WeightInfo};
use pallet_revive::{
	frame_system,
	precompiles::{
		alloy::{self},
		AddressMatcher, Error, Ext, Precompile,
	},
	AddressMapper, ExecOrigin as Origin, H160,
};
use tracing::error;

alloy::sol!("src/interfaces/IConvictionVoting.sol");
use IConvictionVoting::IConvictionVotingCalls;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

type BalanceOf<T> = <<T as pallet_conviction_voting::Config>::Currency as Currency<
	<T as pallet_revive::frame_system::Config>::AccountId,
>>::Balance;

type IndexOf<T> = <<T as pallet_conviction_voting::Config>::Polls as Polling<
	Tally<
		<<T as pallet_conviction_voting::Config>::Currency as Currency<
			<T as pallet_revive::frame_system::Config>::AccountId,
		>>::Balance,
		<T as pallet_conviction_voting::Config>::MaxTurnout,
	>,
>>::Index;

type ClassOf<T> = <<T as pallet_conviction_voting::Config>::Polls as Polling<
	Tally<
		<<T as pallet_conviction_voting::Config>::Currency as Currency<
			<T as pallet_revive::frame_system::Config>::AccountId,
		>>::Balance,
		<T as pallet_conviction_voting::Config>::MaxTurnout,
	>,
>>::Class;

const LOG_TARGET: &str = "conviction-voting::precompiles";

fn revert(error: &impl fmt::Debug, message: &str) -> Error {
	error!(target: LOG_TARGET, ?error, "{}", message);
	Error::Revert(message.into())
}

pub struct ConvictionVotingPrecompile<T>(PhantomData<T>);
impl<T> Precompile for ConvictionVotingPrecompile<T>
where
	T: crate::Config + pallet_revive::Config,
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
				let _ = env.charge(
					<T as Config>::WeightInfo::vote_new()
						.max(<T as Config>::WeightInfo::vote_existing()),
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
				let _ = env.charge(
					<T as Config>::WeightInfo::vote_new()
						.max(<T as Config>::WeightInfo::vote_existing()),
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
				let _ = env.charge(
					<T as Config>::WeightInfo::vote_new()
						.max(<T as Config>::WeightInfo::vote_existing()),
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
				let _ = env
					.charge(<T as Config>::WeightInfo::delegate(<T as Config>::MaxVotes::get()))?;

				let target_account_id = T::AddressMapper::to_account_id(&H160::from(to.0 .0));
				let target_source = T::Lookup::unlookup(target_account_id);

				let runtime_conviction = Self::to_conviction(conviction)?;

				pallet_conviction_voting::Pallet::<T>::delegate(
					frame_origin,
					Self::u16_to_track_id(trackId)?,
					target_source,
					runtime_conviction,
					Self::u128_to_balance(balance)?,
				)
				.map(|_| Vec::new())
				.map_err(|error| revert(&error, "ConvictionVoting: delegation failed"))
			},
			IConvictionVotingCalls::undelegate(IConvictionVoting::undelegateCall { trackId }) => {
				let _ = env.charge(<T as Config>::WeightInfo::undelegate(
					<T as Config>::MaxVotes::get(),
				))?;

				pallet_conviction_voting::Pallet::<T>::undelegate(
					frame_origin,
					Self::u16_to_track_id(trackId)?,
				)
				.map(|_| Vec::new())
				.map_err(|error| revert(&error, "ConvictionVoting: undelegation failed"))
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

	fn u128_to_balance(balance: &u128) -> Result<BalanceOf<T>, Error> {
		(*balance)
			.try_into()
			.map_err(|_| Error::Revert("ConvictionVoting: balance is too large".into()))
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
}
