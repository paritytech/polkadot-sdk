#![cfg(feature = "runtime-benchmarks")]
use super::*;
use alloc::{vec, vec::Vec};
use frame_benchmarking::v2::*;
use frame_support::traits::VoteTally;
use pallet_conviction_voting::{AccountVote, Conviction, Event, TallyOf, Vote};
use pallet_revive::{
	precompiles::alloy::{
		hex,
		sol_types::{SolCall, SolInterface},
		call_builder::{caller_funding, CallSetup}
	},
	run::precompile as run_precompile,
	ExecConfig, ExecReturnValue, Weight, H160,
	BenchmarkSystem
};

/// Fill all classes as much as possible up to `MaxVotes` and return the Class with the most votes
/// ongoing.
fn fill_voting<T: Config>(
) -> (ClassOf<T>, BTreeMap<ClassOf<T>, Vec<IndexOf<T>>>) {
	let mut r = BTreeMap::<ClassOf<T>, Vec<IndexOf<T>>>::new();
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

#[benchmarks(
	where
	T: crate::Config + pallet_revive::Config,
	BalanceOf<T>: TryFrom<u128> + Into<u128>, // balance as u128
	IndexOf<T>: TryFrom<u32> + TryInto<u32>,  // u32 as ReferendumIndex
	ClassOf<T>: TryFrom<u16> + TryInto<u16>,  // u16 as TrackId
)]
mod benchmarks {
	use super::*;

	#[benchmark(pov_mode = Measured)]
	fn vote_new() {
		let caller_address = H160(BenchmarkSystem::<T>::MATCHER.base_address());
		let mapped_caller = T::AddressMapper::to_account_id(caller_address);

		T::VotingHooks::on_vote_worst_case(&mapped_caller);

		let (class, all_polls) = fill_voting::<T, I>();
		let polls = &all_polls[&class];
		let r = polls.len() - 1;

		let v = Vote { aye: true, conviction: Conviction::Locked1x };
		let dummy_vote = AccountVote::Standard {v, 10u128};
		// We need to create existing votes, we skip 1 as we voting new
		for i in polls.iter().skip(1) {
			ConvictionVoting::<T>::vote(mapped_caller.clone().into(), *i, dummy_vote)?;
		}

		let encoded_call = IConvictionVoting::IConvictionVotingCalls::voteSplitAbstain(
			IConvictionVoting::voteSplitAbstainCall {
				referendum_index: 0u32,
				aye: 10u128,
				nay: 10u128,
				abstain: 10u128,
			}
		)
		.abi_encode()

		let mut call_setup = CallSetup::<T>::default();
		let (mut ext, _) = call_setup.ext();

		let result;
		#[block]
		{
			result = run_precompile(
				&mut ext,
				caller_address.as_fixed_bytes(),
				encoded_call,
			);
		}

		assert!(result.is_ok());
	}

	#[benchmark(pov_mode = Measured)]
	fn vote_existing() {
		let caller_address = H160(BenchmarkSystem::<T>::MATCHER.base_address());
		let mapped_caller = T::AddressMapper::to_account_id(caller_address);

		T::VotingHooks::on_vote_worst_case(&mapped_caller);

		let (class, all_polls) = fill_voting::<T, I>();
		let polls = &all_polls[&class];
		let r = polls.len() - 1;

		let v = Vote { aye: true, conviction: Conviction::Locked1x };
		let dummy_vote = AccountVote::Standard {v, 10u128};
		// We need to create existing votes
		for i in polls.iter() {
			ConvictionVoting::<T>::vote(mapped_caller.clone().into(), *i, dummy_vote)?;
		}

		let encoded_call = IConvictionVoting::IConvictionVotingCalls::voteSplitAbstain(
			IConvictionVoting::voteSplitAbstainCall {
				referendum_index: 0u32,
				aye: 10u128,
				nay: 10u128,
				abstain: 10u128,
			}
		)
		.abi_encode()

		let mut call_setup = CallSetup::<T>::default();
		let (mut ext, _) = call_setup.ext();

		let result;
		#[block]
		{
			result = run_precompile(
				&mut ext,
				caller_address.as_fixed_bytes(),
				encoded_call,
			);
		}

		assert!(result.is_ok());
	}

	impl_benchmark_test_suite!(
		crate::Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test
	);
}