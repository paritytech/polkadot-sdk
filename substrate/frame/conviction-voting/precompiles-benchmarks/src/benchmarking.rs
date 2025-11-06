#![cfg(feature = "runtime-benchmarks")]

extern crate alloc;

use super::*;
use alloc::vec::Vec;
use frame_benchmarking::v2::*;
use frame_benchmarking::whitelisted_caller;
use frame_support::traits::{Get, Polling};
use frame_system::RawOrigin;
use pallet_conviction_voting::{AccountVote, BalanceOf, ClassOf, Conviction, IndexOf, Vote};
use pallet_conviction_voting_precompiles::IConvictionVoting;
use frame_support::traits::OriginTrait;
use pallet_revive::{
	precompiles::alloy::{
		hex,
		sol_types::SolInterface,
	},
	H160,
};
use scale_info::prelude::collections::BTreeMap;

use crate::Pallet as ConvictionVotingPrecompilesBenchmarks;
use pallet_conviction_voting::{Pallet as ConvictionVoting, VotingHooks};
use pallet_revive::AddressMapper;
use pallet_revive::ExecReturnValue;
use pallet_revive::U256;
use pallet_revive::Weight;
 use pallet_revive::ExecConfig;

fn call_precompile<T: Config<I>, I: 'static>(
	from: T::AccountId,
	encoded_call: Vec<u8>,
) -> Result<ExecReturnValue, sp_runtime::DispatchError> {
	let precompile_addr = H160::from(
		hex::const_decode_to_array(b"00000000000000000000000000000000000C0000").unwrap(),
	);

	let result = pallet_revive::Pallet::<T>::bare_call(
		<T as frame_system::Config>::RuntimeOrigin::signed(from),
		precompile_addr,
		U256::zero(),
		Weight::MAX,
		T::Balance::try_from(U256::from(u128::MAX)).ok().unwrap(),
		encoded_call,
		ExecConfig::new_substrate_tx(),
	);

	return result.result
}

/// Fill all classes as much as possible up to `MaxVotes` and return the Class with the most votes
/// ongoing.
fn fill_voting<T: Config<I>, I: 'static>(
) -> (ClassOf<T, I>, BTreeMap<ClassOf<T, I>, Vec<IndexOf<T, I>>>) {
	let mut r = BTreeMap::<ClassOf<T, I>, Vec<IndexOf<T, I>>>::new();
	for class in T::Polls::classes().into_iter() {
		for _ in 0..T::MaxVotes::get() {
			match T::Polls::create_ongoing(class.clone()) {
				Ok(i) => r.entry(class.clone()).or_default().push(i),
				Err(()) => break,
			}
		}
	}
	let c = r.iter().max_by_key(|(_, v)| v.len()).unwrap().0.clone();
	(c, r)
}

fn whitelisted_mapped_account<T: Config<I>, I: 'static>() -> (H160, T::AccountId) {
	let caller: T::AccountId = whitelisted_caller();
    (T::AddressMapper::to_address(&caller), caller)
}

#[benchmarks(
	where
	T: crate::Config,
)]
mod benchmarks {
	use super::*;

	#[benchmark(pov_mode = Measured)]
	fn vote_new() {
		let (_, mapped_caller) = whitelisted_mapped_account::<T,()>();

		T::VotingHooks::on_vote_worst_case(&mapped_caller);

		let (class, all_polls) = fill_voting::<T, ()>();
		let polls = &all_polls[&class];

		let vote = Vote { aye: true, conviction: Conviction::Locked1x };
		let balance: BalanceOf<T, ()> = 10u32.into();
		let dummy_vote = AccountVote::Standard { vote, balance };
		// We need to create existing votes, we skip 1 as we voting new
		for i in polls.iter().skip(1) {
			ConvictionVoting::<T>::vote(
				RawOrigin::Signed(mapped_caller.clone()).into(),
				*i,
				dummy_vote,
			)
			.unwrap();
		}

		let encoded_call = IConvictionVoting::IConvictionVotingCalls::voteSplitAbstain(
			IConvictionVoting::voteSplitAbstainCall {
				referendumIndex: 0u32,
				ayeAmount: 10u128,
				nayAmount: 10u128,
				abstainAmount: 10u128,
			},
		)
		.abi_encode();

		let result;
		#[block]
		{
			result = call_precompile::<T,()>(mapped_caller, encoded_call);
		}

		assert!(result.is_ok());
	}

	#[benchmark(pov_mode = Measured)]
	fn vote_existing() {
		let (_, mapped_caller) = whitelisted_mapped_account::<T,()>();

		T::VotingHooks::on_vote_worst_case(&mapped_caller);

		let (class, all_polls) = fill_voting::<T, ()>();
		let polls = &all_polls[&class];

		let vote = Vote { aye: true, conviction: Conviction::Locked1x };
		let balance: BalanceOf<T, ()> = 10u32.into();
		let dummy_vote = AccountVote::Standard { vote, balance };

		// We need to create existing votes
		for i in polls.iter() {
			ConvictionVoting::<T>::vote(
				RawOrigin::Signed(mapped_caller.clone()).into(),
				*i,
				dummy_vote,
			)
			.unwrap();
		}

		let encoded_call = IConvictionVoting::IConvictionVotingCalls::voteSplitAbstain(
			IConvictionVoting::voteSplitAbstainCall {
				referendumIndex: 0u32,
				ayeAmount: 10u128,
				nayAmount: 10u128,
				abstainAmount: 10u128,
			},
		)
		.abi_encode();

		let result;
		#[block]
		{
			result = call_precompile::<T,()>(mapped_caller, encoded_call);
		}

		assert!(result.is_ok());
	}

	impl_benchmark_test_suite!(
		ConvictionVotingPrecompilesBenchmarks,
		crate::mock::new_test_ext(),
		crate::mock::Test
	);
}
