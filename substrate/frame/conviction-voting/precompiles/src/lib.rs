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
	traits::{Currency, Polling},
};
use pallet_conviction_voting::{AccountVote, Config, Conviction, Tally, Vote, WeightInfo};
use pallet_revive::{
	precompiles::{
		alloy::{self},
		AddressMatcher, Error, Ext, Precompile,
	},
	ExecOrigin as Origin,
};
use tracing::error;

alloy::sol!("src/interfaces/IConvictionVoting.sol");
use IConvictionVoting::IConvictionVotingCalls;

const LOG_TARGET: &str = "conviction-voting::precompiles";

fn revert(error: &impl fmt::Debug, message: &str) -> Error {
	error!(target: LOG_TARGET, ?error, "{}", message);
	Error::Revert(message.into())
}

pub struct ConvictionVotingPrecompile<T>(PhantomData<T>);
impl<T> Precompile for ConvictionVotingPrecompile<T>
where
	T: crate::Config + pallet_revive::Config,
	<<T as pallet_conviction_voting::Config>::Polls as Polling<
		Tally<
			<<T as pallet_conviction_voting::Config>::Currency as Currency<
				<T as pallet_revive::frame_system::Config>::AccountId,
			>>::Balance,
			<T as pallet_conviction_voting::Config>::MaxTurnout,
		>,
	>>::Index: From<u32>, //Enforces u32 as ReferendumIndex
	<<T as pallet_conviction_voting::Config>::Currency as Currency<
		<T as pallet_revive::frame_system::Config>::AccountId,
	>>::Balance: From<u128>,
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

				let vote = Vote { aye: *aye, conviction: to_runtime_conviction(*conviction)? };
				let account_vote = AccountVote::Standard { vote, balance: (*balance).into() };

				pallet_conviction_voting::Pallet::<T>::vote(
					frame_origin,
					(*referendumIndex).into(),
					account_vote,
				)
				.map(|_| Vec::new())
				.map_err(|error| revert(&error, "ConvictionVoting: vote failed"))
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

				let account_vote =
					AccountVote::Split { aye: (*ayeAmount).into(), nay: (*nayAmount).into() };

				pallet_conviction_voting::Pallet::<T>::vote(
					frame_origin,
					(*referendumIndex).into(),
					account_vote,
				)
				.map(|_| Vec::new())
				.map_err(|error| revert(&error, "ConvictionVoting: vote failed"))
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
					aye: (*ayeAmount).into(),
					nay: (*nayAmount).into(),
					abstain: (*abstainAmount).into(),
				};

				pallet_conviction_voting::Pallet::<T>::vote(
					frame_origin,
					(*referendumIndex).into(),
					account_vote,
				)
				.map(|_| Vec::new())
				.map_err(|error| revert(&error, "ConvictionVoting: vote failed"))
			},
			_ => Ok(Vec::new()),
		}
	}
}

fn to_runtime_conviction(conviction: IConvictionVoting::Conviction) -> Result<Conviction, Error> {
	match conviction {
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
